import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { resolveBrowserDistOutDir as resolveSpecsBrowserDistOutDir } from '../../../../sdkwork-specs/tools/browser-dist-layout.mjs';

const DEPLOYMENT_PROFILES = new Set(['standalone', 'cloud']);
const LIFECYCLE_ENVIRONMENTS = new Set(['development', 'test', 'staging', 'production']);
const PROFILE_ID_PATTERN = /^(standalone|cloud)\.(development|test|staging|production)$/u;

export const CANONICAL_API_PROXY_PATHS = Object.freeze([
  '/app/v3/api',
  '/backend/v3/api',
  '/openapi.json',
  '/healthz',
  '/readyz',
  '/livez',
  '/metrics',
]);

export function resolveBrowserDistOutDir(environment, deploymentProfile = 'standalone') {
  return resolveSpecsBrowserDistOutDir(environment, deploymentProfile);
}

export function resolveViteRuntimeProfile(mode, processEnv = process.env) {
  const modeMatch = PROFILE_ID_PATTERN.exec(mode);
  const deploymentProfile = firstText(
    processEnv.SDKWORK_DEPLOYMENT_PROFILE,
    processEnv.SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE,
    modeMatch?.[1],
    'standalone',
  );
  const environment = firstText(
    processEnv.SDKWORK_ENVIRONMENT,
    processEnv.SDKWORK_WEBSERVER_ENVIRONMENT,
    modeMatch?.[2],
    LIFECYCLE_ENVIRONMENTS.has(mode) ? mode : undefined,
    'development',
  );

  if (!DEPLOYMENT_PROFILES.has(deploymentProfile)) {
    throw new Error(`unsupported Vite deployment profile: ${deploymentProfile}`);
  }
  if (!LIFECYCLE_ENVIRONMENTS.has(environment)) {
    throw new Error(`unsupported Vite lifecycle environment: ${environment}`);
  }
  if (modeMatch && (modeMatch[1] !== deploymentProfile || modeMatch[2] !== environment)) {
    throw new Error(
      `Vite mode ${mode} conflicts with resolved profile ${deploymentProfile}.${environment}`,
    );
  }

  return {
    deploymentProfile,
    environment,
    profileId: `${deploymentProfile}.${environment}`,
  };
}

export function resolveBrowserDevelopmentServer({
  appRoot,
  deploymentProfile,
  environment,
  processEnv = process.env,
  readText = defaultReadText,
}) {
  const profile = loadParentTopologyProfile({
    appRoot,
    deploymentProfile,
    environment,
    processEnv,
    readText,
  });
  const applicationRoot = normalizeRelativePath(path.relative(profile.repositoryRoot, appRoot));

  // Adaptive Web: one browser-visible ingress (clientProcess.bindEnv) selects
  // PC/H5. Vite renderers bind private INTERNAL ports only — never the public
  // ingress port (APP_RUNTIME_TOPOLOGY_SPEC.md §8.2 / nginx Adaptive Web parity).
  const adaptive = resolveAdaptiveRendererDevelopmentServer({
    profile,
    applicationRoot,
    deploymentProfile,
  });
  if (adaptive) {
    return adaptive;
  }

  const browserProcesses = (profile.orchestration.processes ?? []).filter((processEntry) => (
    processEntry?.role === 'client'
      && normalizeRelativePath(processEntry.applicationRoot) === applicationRoot
      && Array.isArray(processEntry.runtimeTargets)
      && processEntry.runtimeTargets.includes('browser')
  ));
  if (browserProcesses.length !== 1) {
    throw new Error(
      `${profile.profileId} must declare exactly one browser client for ${applicationRoot}`,
    );
  }

  const bindEnv = requireText(browserProcesses[0].bindEnv, 'browser process bindEnv');
  const bind = parseTcpBind(
    requireText(profile.environmentValues[bindEnv], `${profile.profileId}.${bindEnv}`),
    bindEnv,
  );
  let proxyTarget;
  if (deploymentProfile === 'standalone') {
    assertStandaloneBrowserDelivery(
      profile.orchestration,
      browserProcesses[0],
      applicationRoot,
      profile.profileId,
    );
    proxyTarget = resolveStandaloneProxyTarget(profile);
  }

  return {
    ...bind,
    profileId: profile.profileId,
    proxyTarget,
  };
}

function resolveAdaptiveRendererDevelopmentServer({
  profile,
  applicationRoot,
  deploymentProfile,
}) {
  const deliveries = (profile.orchestration.browserDeliveries ?? []).filter((delivery) => (
    delivery?.deliveryMode === 'dev-server-proxy'
      && delivery?.renderers
      && typeof delivery.renderers === 'object'
  ));
  for (const delivery of deliveries) {
    for (const [architecture, renderer] of Object.entries(delivery.renderers)) {
      if (normalizeRelativePath(renderer?.applicationRoot) !== applicationRoot) {
        continue;
      }
      if (
        delivery.originMode !== 'same-origin'
          || delivery.preserveCanonicalPaths !== true
      ) {
        throw new Error(
          `${profile.profileId} adaptive delivery ${delivery.id} must be a canonical-path same-origin dev-server proxy`,
        );
      }
      const portEnv = firstText(renderer.portEnv);
      const portValue = firstText(
        portEnv ? profile.environmentValues[portEnv] : undefined,
        renderer.defaultPort !== undefined ? String(renderer.defaultPort) : undefined,
      );
      if (!portValue) {
        throw new Error(
          `${profile.profileId} adaptive renderer ${architecture} for ${applicationRoot} requires portEnv or defaultPort`,
        );
      }
      const host = firstText(
        renderer.hostEnv ? profile.environmentValues[renderer.hostEnv] : undefined,
        '127.0.0.1',
      );
      const bind = parseTcpBind(`${host}:${portValue}`, `${architecture} renderer`);
      return {
        ...bind,
        profileId: profile.profileId,
        proxyTarget: deploymentProfile === 'standalone'
          ? resolveStandaloneProxyTarget(profile)
          : undefined,
        adaptive: true,
        architecture,
      };
    }
  }
  return undefined;
}

function resolveStandaloneProxyTarget(profile) {
  const ingress = profile.topology.surfaces?.['application.public-ingress'];
  const ingressUrlEnv = requireText(
    ingress?.httpUrlEnv,
    'application.public-ingress.httpUrlEnv',
  );
  return parseHttpOrigin(
    requireText(
      profile.environmentValues[ingressUrlEnv],
      `${profile.profileId}.${ingressUrlEnv}`,
    ),
    ingressUrlEnv,
  );
}

function assertStandaloneBrowserDelivery(orchestration, browserProcess, applicationRoot, profileId) {
  const deliveries = (orchestration.browserDeliveries ?? []).filter((delivery) => (
    delivery?.applicationRoot === applicationRoot
      && delivery?.clientProcessId === browserProcess.id
  ));
  if (deliveries.length !== 1) {
    throw new Error(
      `${profileId} must declare exactly one browser delivery for ${browserProcess.id}`,
    );
  }
  const [delivery] = deliveries;
  if (
    delivery.originMode !== 'same-origin'
      || delivery.deliveryMode !== 'dev-server-proxy'
      || delivery.apiSurfaceId !== 'application.public-ingress'
      || delivery.preserveCanonicalPaths !== true
      || !sameStringSet(delivery.clientArchitectures, browserProcess.clientArchitectures)
  ) {
    throw new Error(
      `${profileId} browser delivery for ${browserProcess.id} must be a canonical-path same-origin dev-server proxy to application.public-ingress`,
    );
  }
}

export function createCanonicalApiProxyConfig(target) {
  const normalizedTarget = parseHttpOrigin(target, 'standalone proxy target');
  return Object.fromEntries(CANONICAL_API_PROXY_PATHS.map((canonicalPath) => [
    canonicalPath,
    {
      changeOrigin: false,
      target: normalizedTarget,
    },
  ]));
}

function loadParentTopologyProfile({
  appRoot,
  deploymentProfile,
  environment,
  processEnv,
  readText,
}) {
  const profileId = `${deploymentProfile}.${environment}`;
  const deploymentConfigPath = path.join(appRoot, 'etc', 'sdkwork.deployment.config.json');
  const deployment = readJson(deploymentConfigPath, readText);
  const repositoryRoot = path.resolve(appRoot, '..', '..');
  const parentDeploymentConfigPath = resolveInside(
    repositoryRoot,
    path.dirname(deploymentConfigPath),
    deployment.parentDeploymentConfig,
    'parentDeploymentConfig',
  );
  const parentTopologySpecPath = resolveInside(
    repositoryRoot,
    path.dirname(deploymentConfigPath),
    deployment.parentTopologySpec,
    'parentTopologySpec',
  );
  const parentDeployment = readJson(parentDeploymentConfigPath, readText);
  const topology = readJson(parentTopologySpecPath, readText);
  if (topology.schemaVersion !== 5 || topology.kind !== 'sdkwork.app.topology') {
    throw new Error('parentTopologySpec must reference an sdkwork.app.topology v5 contract');
  }

  const profileConfig = parentDeployment.profiles?.[profileId]?.config;
  const profilePath = resolveInside(
    path.dirname(parentDeploymentConfigPath),
    path.dirname(parentDeploymentConfigPath),
    profileConfig,
    `parent profile ${profileId}`,
  );
  if (!existsSync(profilePath)) {
    throw new Error(`parent profile does not exist: ${profilePath}`);
  }
  const environmentValues = {
    ...parseEnvironmentFile(readText(profilePath)),
    ...stringEnvironment(processEnv),
  };
  assertProfileIdentity(environmentValues, deploymentProfile, environment, profileId);

  const orchestration = topology.orchestration?.profiles?.[profileId];
  if (!orchestration) {
    throw new Error(`parent topology does not declare orchestration profile ${profileId}`);
  }

  return {
    environmentValues,
    orchestration,
    profileId,
    repositoryRoot,
    topology,
  };
}

function assertProfileIdentity(values, deploymentProfile, environment, profileId) {
  const actualDeploymentProfile = firstText(
    values.SDKWORK_DEPLOYMENT_PROFILE,
    values.SDKWORK_WEBSERVER_DEPLOYMENT_PROFILE,
  );
  const actualEnvironment = firstText(
    values.SDKWORK_ENVIRONMENT,
    values.SDKWORK_WEBSERVER_ENVIRONMENT,
  );
  const actualProfileId = firstText(values.SDKWORK_PROFILE_ID, values.SDKWORK_WEBSERVER_PROFILE_ID);
  if (
    actualDeploymentProfile !== deploymentProfile
      || actualEnvironment !== environment
      || actualProfileId !== profileId
  ) {
    throw new Error(`parent topology profile identity does not match ${profileId}`);
  }
}

function parseEnvironmentFile(source) {
  const result = {};
  for (const [index, sourceLine] of source.split(/\r?\n/u).entries()) {
    const line = sourceLine.trim();
    if (!line || line.startsWith('#')) continue;
    const separator = line.indexOf('=');
    if (separator <= 0) {
      throw new Error(`invalid topology env line ${index + 1}`);
    }
    const key = line.slice(0, separator).trim();
    if (!/^[A-Z][A-Z0-9_]*$/u.test(key)) {
      throw new Error(`invalid topology env key on line ${index + 1}: ${key}`);
    }
    result[key] = parseEnvironmentValue(line.slice(separator + 1).trim());
  }
  return result;
}

function parseEnvironmentValue(value) {
  if (
    value.length >= 2
      && ((value.startsWith('"') && value.endsWith('"'))
        || (value.startsWith("'") && value.endsWith("'")))
  ) {
    return value.slice(1, -1);
  }
  return value.replace(/\s+#.*$/u, '').trim();
}

function parseTcpBind(value, field) {
  let url;
  try {
    url = new URL(`http://${value}`);
  } catch {
    throw new Error(`${field} must be a host:port TCP binding`);
  }
  const port = Number(url.port);
  if (
    !url.hostname
      || !Number.isInteger(port)
      || port < 1
      || port > 65535
      || url.pathname !== '/'
      || url.search
      || url.hash
  ) {
    throw new Error(`${field} must be a host:port TCP binding`);
  }
  return {
    host: url.hostname.replace(/^\[|\]$/gu, ''),
    port,
  };
}

function parseHttpOrigin(value, field) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new Error(`${field} must be an absolute HTTP(S) origin`);
  }
  if (
    !['http:', 'https:'].includes(url.protocol)
      || url.username
      || url.password
      || url.pathname !== '/'
      || url.search
      || url.hash
  ) {
    throw new Error(`${field} must be an absolute HTTP(S) origin`);
  }
  return url.origin;
}

function resolveInside(boundary, base, relativePath, field) {
  const value = requireText(relativePath, field);
  if (path.isAbsolute(value)) {
    throw new Error(`${field} must be a relative path`);
  }
  const resolved = path.resolve(base, value);
  const relative = path.relative(boundary, resolved);
  if (relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) {
    throw new Error(`${field} must stay inside ${boundary}`);
  }
  return resolved;
}

function readJson(file, readText) {
  try {
    return JSON.parse(readText(file));
  } catch (error) {
    throw new Error(`${file} is not valid JSON: ${error instanceof Error ? error.message : String(error)}`);
  }
}

function defaultReadText(file) {
  return readFileSync(file, 'utf8');
}

function stringEnvironment(environment) {
  return Object.fromEntries(
    Object.entries(environment).filter((entry) => typeof entry[1] === 'string'),
  );
}

function normalizeRelativePath(value) {
  return typeof value === 'string' ? value.replaceAll('\\', '/').replace(/^\.\//u, '') : '';
}

function sameStringSet(left, right) {
  return Array.isArray(left)
    && Array.isArray(right)
    && left.length === right.length
    && left.every((value) => right.includes(value));
}

function firstText(...values) {
  return values.find((value) => typeof value === 'string' && value.trim())?.trim();
}

function requireText(value, field) {
  const normalized = firstText(value);
  if (!normalized) throw new Error(`${field} is required`);
  return normalized;
}
