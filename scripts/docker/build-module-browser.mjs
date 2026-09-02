#!/usr/bin/env node

import { existsSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

import {
  buildBrowserClient,
  discoverBrowserAppRoots,
  normalizeEnvironmentAlias,
} from '../../../sdkwork-specs/tools/build-browser-client.mjs';
import { ensureBuildAccessToken } from '../../../sdkwork-specs/tools/ensure-build-access-token.mjs';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const WORKSPACE_ROOT = path.resolve(REPO_ROOT, '..');

export const DEPLOYMENT_ENVIRONMENT_ALIASES = Object.freeze({
  development: 'dev',
  test: 'test',
  staging: 'staging',
  production: 'prod',
});

export function resolveSpaceCheckoutRoot(explicitCheckoutRoot) {
  if (explicitCheckoutRoot) {
    return path.resolve(explicitCheckoutRoot);
  }
  if (process.env.SDKWORK_SPACE_ROOT) {
    return path.resolve(process.env.SDKWORK_SPACE_ROOT, 'sdkwork-space');
  }
  return WORKSPACE_ROOT;
}

export function resolveModuleRoot(module, spaceCheckoutRoot = resolveSpaceCheckoutRoot()) {
  const normalized = String(module ?? '').trim().replace(/^\/*/, '');
  if (!normalized.startsWith('sdkwork-')) {
    throw new Error('module must be a sibling repository name under the workspace checkout root');
  }
  return path.join(spaceCheckoutRoot, normalized);
}

export function resolveArchitectures(moduleRoot, architecture) {
  const token = String(architecture ?? '').trim();
  if (token === 'all') {
    const discovered = [...new Set(discoverBrowserAppRoots(moduleRoot).map((entry) => entry.architecture))];
    if (discovered.length === 0) {
      throw new Error(`no pc/h5 browser surfaces found under ${moduleRoot}/apps`);
    }
    return discovered;
  }
  if (token !== 'pc' && token !== 'h5') {
    throw new Error('architecture must be pc, h5, or all');
  }
  resolveBrowserAppRootCompat(moduleRoot, token);
  return [token];
}

function resolveBrowserAppRootCompat(moduleRoot, architecture) {
  const matches = discoverBrowserAppRoots(moduleRoot).filter((entry) => entry.architecture === architecture);
  if (matches.length === 0) {
    throw new Error(`module has no ${architecture} browser surface under ${moduleRoot}/apps`);
  }
  return matches[0];
}

export function buildEnvironmentAlias(deploymentEnvironment, explicitEnvironment) {
  if (explicitEnvironment) {
    return normalizeEnvironmentAlias(explicitEnvironment);
  }
  const alias = DEPLOYMENT_ENVIRONMENT_ALIASES[String(deploymentEnvironment ?? '').trim()];
  if (!alias) {
    throw new Error('deployment-environment must be development, test, staging, or production when --environment is omitted');
  }
  return normalizeEnvironmentAlias(alias);
}

export async function buildModuleBrowser(options) {
  const module = String(options.module ?? '').trim();
  const deploymentProfile = String(options.deploymentProfile ?? 'standalone').trim();
  const deploymentEnvironment = String(options.deploymentEnvironment ?? 'development').trim();
  const spaceCheckoutRoot = resolveSpaceCheckoutRoot(options.spaceCheckoutRoot ?? options.spaceRoot);
  const moduleRoot = resolveModuleRoot(module, spaceCheckoutRoot);
  if (!existsSync(moduleRoot)) {
    throw new Error(`module checkout missing: ${moduleRoot}`);
  }

  const environment = buildEnvironmentAlias(deploymentEnvironment, options.environment);
  const architectures = resolveArchitectures(moduleRoot, options.architecture);
  const plans = [];

  for (const architecture of architectures) {
    // Seed SDKWORK_ACCESS_TOKEN before the shared build runner so both the
    // host process and Vite credential-entry plugin see the same bootstrap
    // credential (ENVIRONMENT_SPEC §6.1). buildBrowserClient also ensures the
    // token; this pre-seed keeps the lifecycle environment explicit.
    const appRoot = resolveBrowserAppRootCompat(moduleRoot, architecture).root;
    try {
      const token = await ensureBuildAccessToken({
        allowTestTokenGeneration: true,
        appRoot,
        environment: deploymentEnvironment,
      });
      if (token) {
        process.env.SDKWORK_ACCESS_TOKEN = token;
      } else if (deploymentEnvironment === 'development' || deploymentEnvironment === 'test') {
        console.warn(
          `[build-module-browser] SDKWORK_ACCESS_TOKEN empty for ${module} ${architecture} ${deploymentProfile}.${deploymentEnvironment}`,
        );
      }
    } catch (error) {
      console.warn(
        `[build-module-browser] bootstrap access token unavailable for ${module}/${architecture}: ${
          error instanceof Error ? error.message : String(error)
        }`,
      );
    }
    const plan = await buildBrowserClient({
      architecture,
      deploymentProfile,
      dryRun: options.dryRun === true,
      environment,
      repositoryRoot: moduleRoot,
    });
    plans.push({
      architecture,
      environment,
      module,
      moduleRoot,
      ...plan,
    });
  }

  return {
    deploymentEnvironment,
    deploymentProfile,
    environment,
    module,
    moduleRoot,
    plans,
    spaceCheckoutRoot,
  };
}

function parseSettings(argv) {
  const { values } = parseArgs({
    args: argv,
    options: {
      architecture: { type: 'string', default: 'all' },
      'deployment-environment': { type: 'string', default: 'development' },
      'deployment-profile': { type: 'string', default: 'standalone' },
      'dry-run': { type: 'boolean', default: false },
      environment: { type: 'string' },
      help: { type: 'boolean', short: 'h' },
      module: { type: 'string' },
    },
  });
  if (values.help) {
    console.log(`Usage: node scripts/docker/build-module-browser.mjs --module <sdkwork-module> [--architecture pc|h5|all] [--environment dev|test|staging|prod] [--deployment-environment development|test|staging|production] [--deployment-profile standalone|cloud] [--dry-run]

Build one sibling module's Adaptive Web browser client on the host checkout.
When --architecture all (default), every owned pc/h5 surface for the module is rebuilt.
Checkout root: ${WORKSPACE_ROOT}`);
    process.exit(0);
  }
  if (!values.module) {
    throw new Error('--module is required');
  }
  return {
    architecture: values.architecture,
    deploymentEnvironment: values['deployment-environment'],
    deploymentProfile: values['deployment-profile'],
    dryRun: values['dry-run'],
    environment: values.environment,
    module: values.module,
  };
}

async function main() {
  const settings = parseSettings(process.argv.slice(2));
  const result = await buildModuleBrowser(settings);
  console.log(JSON.stringify(result, null, 2));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}
