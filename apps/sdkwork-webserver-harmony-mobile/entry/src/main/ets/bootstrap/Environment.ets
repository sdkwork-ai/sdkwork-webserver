/**
 * HarmonyOS runtime environment resolution for the Web Server HarmonyOS root.
 *
 * Authority: `config/app/runtime-env.<deploymentProfile>.<environment>.json`
 * (`CONFIG_SPEC.md`, `ENVIRONMENT_SPEC.md`, and
 * `HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §9). The build projects exactly one
 * selected non-secret runtime JSON resource into the HAP; this module only reads
 * and validates it.
 *
 * Every failure mode throws with the field name instead of falling back to a
 * default host, so a mis-selected profile fails the launch rather than pointing
 * the console at the wrong backend.
 */

export type WebserverHarmonyRuntimeTarget = 'harmony-native';
export type WebserverHarmonyLifecycleEnvironment =
  | 'development'
  | 'test'
  | 'staging'
  | 'demo'
  | 'production';
export type WebserverHarmonyDeploymentProfile = 'standalone';

export interface WebserverHarmonyEnvironment {
  readonly environment: WebserverHarmonyLifecycleEnvironment;
  readonly deploymentProfile: WebserverHarmonyDeploymentProfile;
  readonly profileId: string;
  readonly runtimeTarget: WebserverHarmonyRuntimeTarget;
  readonly webserverApiBaseUrl: string;
  readonly webserverBackendApiBaseUrl: string;
  readonly applicationPublicHttpUrl: string;
  readonly appbaseAppApiBaseUrl: string;
  readonly appbaseLoginUrl: string;
}

const APP_API_SUFFIX: string = '/app/v3/api';
const BACKEND_API_SUFFIX: string = '/backend/v3/api';
const ENVIRONMENTS: string[] = ['development', 'test', 'staging', 'demo', 'production'];

function requireNonEmpty(value: string | undefined, key: string): string {
  const normalized: string = value === undefined ? '' : value.trim();
  if (normalized.length === 0) {
    throw new Error(`${key} is required by the HarmonyOS runtime config`);
  }
  return normalized;
}

function requireOneOf(value: string | undefined, allowed: string[], key: string): string {
  const normalized: string = requireNonEmpty(value, key);
  if (!allowed.includes(normalized)) {
    throw new Error(`${key} must be one of ${allowed.join(', ')}`);
  }
  return normalized;
}

function stripTrailingSlashes(value: string): string {
  let normalized: string = value.trim();
  while (normalized.endsWith('/')) {
    normalized = normalized.slice(0, normalized.length - 1);
  }
  return normalized;
}

/**
 * Assert an absolute HTTP(S) URL that ends with `<suffix>` exactly once.
 *
 * The generated webserver app SDK appends the surface prefix internally
 * (`ApiPaths.appPath`), so the runtime value must already carry `/app/v3/api` —
 * the opposite of the mini program root, whose base URL is a bare origin.
 */
export function normalizeWebserverHarmonyAppApiBaseUrl(value: string): string {
  const normalized: string = stripTrailingSlashes(requireNonEmpty(value, 'webserver.apiBaseUrl'));
  if (!/^https?:\/\//u.test(normalized)) {
    throw new Error('webserver.apiBaseUrl must be an absolute HTTP(S) URL');
  }
  if (!normalized.endsWith(APP_API_SUFFIX)) {
    throw new Error(`webserver.apiBaseUrl must end with ${APP_API_SUFFIX}`);
  }
  const prefix: string = normalized.slice(0, normalized.length - APP_API_SUFFIX.length);
  if (prefix.endsWith(APP_API_SUFFIX)) {
    throw new Error(`webserver.apiBaseUrl must contain ${APP_API_SUFFIX} exactly once`);
  }
  return normalized;
}

export function normalizeWebserverHarmonyBackendApiBaseUrl(value: string): string {
  const normalized: string = stripTrailingSlashes(
    requireNonEmpty(value, 'webserver.backendApiBaseUrl'),
  );
  if (!/^https?:\/\//u.test(normalized)) {
    throw new Error('webserver.backendApiBaseUrl must be an absolute HTTP(S) URL');
  }
  if (!normalized.endsWith(BACKEND_API_SUFFIX)) {
    throw new Error(`webserver.backendApiBaseUrl must end with ${BACKEND_API_SUFFIX}`);
  }
  return normalized;
}

function normalizeOrigin(value: string | undefined, key: string): string {
  const normalized: string = stripTrailingSlashes(requireNonEmpty(value, key));
  if (!/^https?:\/\//u.test(normalized)) {
    throw new Error(`${key} must be an absolute HTTP(S) URL`);
  }
  return normalized;
}

function readGroup(source: Record<string, Object>, key: string): Record<string, Object> {
  const value: Object = source[key];
  if (value === undefined || value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${key} must be an object in the HarmonyOS runtime config`);
  }
  return value as Record<string, Object>;
}

export function createWebserverHarmonyEnvironment(
  runtimeConfig: Record<string, Object>,
): WebserverHarmonyEnvironment {
  const webserver: Record<string, Object> = readGroup(runtimeConfig, 'webserver');
  const appbase: Record<string, Object> = readGroup(runtimeConfig, 'appbase');
  const target: string = requireNonEmpty(runtimeConfig.runtimeTarget as string, 'runtimeTarget');
  if (target !== 'harmony-native') {
    throw new Error('runtimeTarget must be harmony-native for this application root');
  }
  const environment: string = requireOneOf(
    runtimeConfig.environment as string,
    ENVIRONMENTS,
    'environment',
  );
  const deploymentProfile: string = requireNonEmpty(
    runtimeConfig.deploymentProfile as string,
    'deploymentProfile',
  );
  const profileId: string = requireNonEmpty(runtimeConfig.profileId as string, 'profileId');
  if (profileId !== `${deploymentProfile}.${environment}`) {
    throw new Error(`profileId must equal ${deploymentProfile}.${environment}`);
  }
  const metadata: Object = runtimeConfig.metadata;
  const applicationPublicHttpUrl: string = normalizeOrigin(
    metadata === undefined || metadata === null
      ? undefined
      : (metadata as Record<string, Object>).applicationPublicHttpUrl as string,
    'metadata.applicationPublicHttpUrl',
  );
  return {
    environment: environment as WebserverHarmonyLifecycleEnvironment,
    deploymentProfile: deploymentProfile as WebserverHarmonyDeploymentProfile,
    profileId,
    runtimeTarget: 'harmony-native',
    webserverApiBaseUrl: normalizeWebserverHarmonyAppApiBaseUrl(webserver.apiBaseUrl as string),
    webserverBackendApiBaseUrl: normalizeWebserverHarmonyBackendApiBaseUrl(
      webserver.backendApiBaseUrl as string,
    ),
    applicationPublicHttpUrl,
    appbaseAppApiBaseUrl: normalizeOrigin(appbase.appApiBaseUrl as string, 'appbase.appApiBaseUrl'),
    appbaseLoginUrl: normalizeOrigin(appbase.loginUrl as string, 'appbase.loginUrl'),
  };
}
