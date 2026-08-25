#!/usr/bin/env node

import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const APP_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const DEPLOYMENT_CONFIG_PATH = path.join(APP_ROOT, 'etc', 'sdkwork.deployment.config.json');
const SUPPORTED_ENVIRONMENTS = ['development', 'test', 'staging', 'production'];
const SUPPORTED_PROFILES = ['standalone', 'cloud'];
const PROFILE_MATRIX = SUPPORTED_PROFILES.flatMap((profile) =>
  SUPPORTED_ENVIRONMENTS.map((environment) => `${profile}.${environment}`),
);
const SDK_BASE_URL_KEYS = [
  'appApiBaseUrl',
  'backendApiBaseUrl',
  'driveAppApiBaseUrl',
  'appbaseAppApiBaseUrl',
  'deployAppApiBaseUrl',
];
const NAVIGATION_URL_KEYS = ['messagingPcUrl'];

function option(argv, name, fallback) {
  const index = argv.indexOf(name);
  return index >= 0 ? argv[index + 1] : fallback;
}

function normalizeOrigin(url) {
  return new URL(url).origin;
}

function loadParentDeploymentConfig() {
  const deployment = JSON.parse(readFileSync(DEPLOYMENT_CONFIG_PATH, 'utf8'));
  const parentRelative = deployment.parentDeploymentConfig;
  if (typeof parentRelative !== 'string' || parentRelative.length === 0) {
    throw new Error('deployment config does not declare parentDeploymentConfig');
  }
  const parentPath = path.resolve(path.dirname(DEPLOYMENT_CONFIG_PATH), parentRelative);
  if (!existsSync(parentPath)) {
    throw new Error(`parent deployment config does not exist: ${parentPath}`);
  }
  return {
    path: parentPath,
    value: JSON.parse(readFileSync(parentPath, 'utf8')),
  };
}

function resolveCloudApiBaseUrl(parent, environment) {
  const value = parent.value.environments?.[environment]?.cloudApiBaseUrl;
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(
      `parent deployment config must declare environments.${environment}.cloudApiBaseUrl`,
    );
  }
  return normalizeOrigin(value);
}

export function resolveSource(deploymentProfile, environment) {
  const profileId = `${deploymentProfile}.${environment}`;
  const deployment = JSON.parse(readFileSync(DEPLOYMENT_CONFIG_PATH, 'utf8'));
  const sourcePath = deployment.profiles?.[profileId]?.source;
  if (typeof sourcePath !== 'string' || sourcePath.length === 0) {
    throw new Error(`deployment config does not declare browser source for ${profileId}`);
  }
  const source = path.resolve(path.dirname(DEPLOYMENT_CONFIG_PATH), sourcePath);
  if (!existsSync(source)) {
    throw new Error(`browser runtime source does not exist for ${profileId}: ${sourcePath}`);
  }
  const value = JSON.parse(readFileSync(source, 'utf8'));
  validateRuntimeSource(value, {
    cloudApiBaseUrl: resolveCloudApiBaseUrl(loadParentDeploymentConfig(), environment),
    deploymentProfile,
    environment,
    sourcePath,
  });
  return { path: source, value };
}

export function assertFullProfileMatrix() {
  const deployment = JSON.parse(readFileSync(DEPLOYMENT_CONFIG_PATH, 'utf8'));
  const profiles = deployment.profiles ?? {};
  const missing = PROFILE_MATRIX.filter((profileId) => typeof profiles[profileId]?.source !== 'string');
  if (missing.length > 0) {
    throw new Error(`deployment config must declare browser sources for all ${PROFILE_MATRIX.length} profiles; missing: ${missing.join(', ')}`);
  }
}

export function validateRuntimeSource(value, {
  cloudApiBaseUrl,
  deploymentProfile,
  environment,
  sourcePath = '<runtime-source>',
}) {
  const profileId = `${deploymentProfile}.${environment}`;
  if (value.deploymentProfile !== deploymentProfile || value.environment !== environment) {
    throw new Error(`browser runtime source identity does not match ${profileId}: ${sourcePath}`);
  }
  if (value.profileId !== profileId) {
    throw new Error(`browser runtime source profileId must equal ${profileId}: ${sourcePath}`);
  }
  if (value.runtimeTarget !== 'browser') {
    throw new Error(`browser runtime source runtimeTarget must equal browser: ${sourcePath}`);
  }
  if (deploymentProfile === 'standalone') {
    if (value.browserOriginMode !== 'same-origin') {
      throw new Error(`${profileId}.browserOriginMode must equal same-origin`);
    }
    for (const key of SDK_BASE_URL_KEYS) {
      if (value[key] !== '/') {
        throw new Error(`${profileId}.${key} must use the canonical same-origin root /`);
      }
    }
  } else {
    if (value.browserOriginMode !== 'cross-origin') {
      throw new Error(`${profileId}.browserOriginMode must equal cross-origin`);
    }
    for (const key of SDK_BASE_URL_KEYS) {
      validateAbsoluteHttpUrl(value[key], `${profileId}.${key}`);
      const origin = normalizeOrigin(value[key]);
      if (origin !== cloudApiBaseUrl) {
        throw new Error(
          `${profileId}.${key} must equal the unified cloud API edge ${cloudApiBaseUrl} (ENVIRONMENT_SPEC §5.1.0.1), not ${origin}`,
        );
      }
    }
  }
  for (const key of NAVIGATION_URL_KEYS) validateAbsoluteHttpUrl(value[key], `${profileId}.${key}`, environment);
}

function validateAbsoluteHttpUrl(value, field, environment) {
  if (typeof value !== 'string' || value.length === 0) {
    throw new Error(`${field} must be an absolute HTTP(S) URL`);
  }
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new Error(`${field} must be an absolute HTTP(S) URL`);
  }
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) {
    throw new Error(`${field} must be an absolute HTTP(S) URL`);
  }
  if (environment === 'production' && ['localhost', '127.0.0.1', '::1'].includes(url.hostname)) {
    throw new Error(`${field} cannot use a loopback host in production`);
  }
}

function main() {
  const argv = process.argv.slice(2);
  const deploymentProfile = option(argv, '--deployment-profile', 'standalone');
  const environment = option(argv, '--environment', 'development');
  const check = argv.includes('--check');
  if (!SUPPORTED_PROFILES.includes(deploymentProfile)) {
    throw new Error(`unsupported deployment profile: ${deploymentProfile}`);
  }
  if (!SUPPORTED_ENVIRONMENTS.includes(environment)) {
    throw new Error(`unsupported environment: ${environment}`);
  }

  assertFullProfileMatrix();
  const source = resolveSource(deploymentProfile, environment);
  const deployment = JSON.parse(readFileSync(DEPLOYMENT_CONFIG_PATH, 'utf8'));
  const outputPath = deployment.materialization?.output;
  if (typeof outputPath !== 'string' || outputPath.length === 0) {
    throw new Error('deployment config materialization.output is required');
  }
  const output = path.resolve(path.dirname(DEPLOYMENT_CONFIG_PATH), outputPath);
  const desired = `${JSON.stringify(source.value, null, 2)}\n`;
  if (check) {
    const current = existsSync(output) ? readFileSync(output, 'utf8').replace(/\r\n/g, '\n') : null;
    if (current !== desired) {
      throw new Error(`public/runtime-env.json is stale for ${deploymentProfile}.${environment}`);
    }
    console.log(`[sdkwork-webserver-h5] runtime env current: ${deploymentProfile}.${environment}`);
    return;
  }

  mkdirSync(path.dirname(output), { recursive: true });
  writeFileSync(output, desired, 'utf8');
  console.log(
    `[sdkwork-webserver-h5] materialized ${deploymentProfile}.${environment} from ${path.relative(APP_ROOT, source.path).replaceAll('\\', '/')}`,
  );
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    console.error(`[sdkwork-webserver-h5] ${error instanceof Error ? error.message : String(error)}`);
    process.exitCode = 1;
  }
}
