/**
 * Mini program runtime configuration for the Web Server mini program root.
 *
 * The committed profile `config/mini-program/runtime-env.<profileId>.json` is
 * this deployable root's runtime authority
 * (`MINI_PROGRAM_APP_ARCHITECTURE_SPEC.md` §10). `scripts/build-runtime.mjs`
 * validates it and emits `src/runtime/runtime-env.js`, which `src/app.js`
 * requires; nothing in the bundle reads a hardcoded host.
 *
 * Unlike the browser roots there is no page protocol to align to and no
 * same-origin `/` base: a mini program always issues an absolute request against
 * a whitelisted HTTPS domain, so every base URL here MUST be an absolute HTTP(S)
 * URL. Production additionally rejects loopback hosts, matching the browser
 * root's profile validation.
 */

/** Runtime env key names, in one place so the build script and parser agree. */
export const WEBSERVER_MP_RUNTIME_ENV_KEYS = {
  environment: "SDKWORK_ENVIRONMENT",
  deploymentProfile: "SDKWORK_DEPLOYMENT_PROFILE",
  profileId: "SDKWORK_PROFILE_ID",
  runtimeTarget: "SDKWORK_RUNTIME_TARGET",
  appApiBaseUrl: "SDKWORK_WEBSERVER_APP_API_BASE_URL",
  deployAppApiBaseUrl: "SDKWORK_WEBSERVER_DEPLOY_APP_API_BASE_URL",
  driveAppApiBaseUrl: "SDKWORK_WEBSERVER_DRIVE_APP_API_BASE_URL",
  applicationPublicHttpUrl: "SDKWORK_WEBSERVER_APPLICATION_PUBLIC_HTTP_URL",
  defaultLocale: "SDKWORK_WEBSERVER_DEFAULT_LOCALE",
  fallbackLocale: "SDKWORK_WEBSERVER_FALLBACK_LOCALE",
  supportedLocales: "SDKWORK_WEBSERVER_SUPPORTED_LOCALES",
} as const;

export type WebserverMpLifecycleEnvironment =
  | "development"
  | "test"
  | "staging"
  | "demo"
  | "production";
export type WebserverMpDeploymentProfile = "standalone" | "cloud";
export type WebserverMpLocale = "en-US" | "zh-CN";

export interface WebserverMpRuntimeConfig {
  activeLocales: WebserverMpLocale[];
  appApiBaseUrl: string;
  applicationPublicHttpUrl: string;
  defaultLocale: WebserverMpLocale;
  deployAppApiBaseUrl: string;
  deploymentProfile: WebserverMpDeploymentProfile;
  driveAppApiBaseUrl: string;
  environment: WebserverMpLifecycleEnvironment;
  fallbackLocale: WebserverMpLocale;
  profileId: string;
  runtimeTarget: "mini-program";
  supportedLocales: WebserverMpLocale[];
}

const ENVIRONMENTS = ["development", "test", "staging", "demo", "production"] as const;
const PROFILES = ["standalone", "cloud"] as const;
const LOCALES = ["en-US", "zh-CN"] as const;

/**
 * Parse the selected runtime profile. Every failure mode throws with the field
 * name instead of falling back to a default host, so a mis-selected profile
 * fails the build rather than shipping a console pointed at the wrong backend.
 */
export function parseWebserverMpRuntimeConfig(
  value: unknown,
  runtimeProfile?: WebserverMpDeploymentProfile,
): WebserverMpRuntimeConfig {
  if (!isRecord(value)) {
    throw new Error("Mini program runtime configuration must be an object");
  }
  const keys = WEBSERVER_MP_RUNTIME_ENV_KEYS;
  const environment = readEnum(value[keys.environment], ENVIRONMENTS, keys.environment);
  const deploymentProfile = readEnum(
    value[keys.deploymentProfile],
    PROFILES,
    keys.deploymentProfile,
  );
  if (runtimeProfile && deploymentProfile !== runtimeProfile) {
    throw new Error(
      `${keys.deploymentProfile} must equal the selected write-time deployment profile ${runtimeProfile}`,
    );
  }
  const expectedProfileId = `${deploymentProfile}.${environment}`;
  if (value[keys.profileId] !== expectedProfileId) {
    throw new Error(`${keys.profileId} must equal ${expectedProfileId}`);
  }
  if (value[keys.runtimeTarget] !== "mini-program") {
    throw new Error(`${keys.runtimeTarget} must equal mini-program`);
  }

  const supportedLocales = readLocales(value[keys.supportedLocales], keys.supportedLocales);
  const defaultLocale = readEnum(value[keys.defaultLocale], LOCALES, keys.defaultLocale);
  const fallbackLocale = readEnum(value[keys.fallbackLocale], LOCALES, keys.fallbackLocale);
  if (!supportedLocales.includes(defaultLocale) || !supportedLocales.includes(fallbackLocale)) {
    throw new Error("Locale configuration is inconsistent");
  }

  return {
    // A mini program ships one selected locale set; `activeLocales` mirrors it.
    activeLocales: [...supportedLocales],
    appApiBaseUrl: readAbsoluteHttpUrl(
      value[keys.appApiBaseUrl],
      keys.appApiBaseUrl,
      environment,
    ),
    applicationPublicHttpUrl: readAbsoluteHttpUrl(
      value[keys.applicationPublicHttpUrl],
      keys.applicationPublicHttpUrl,
      environment,
    ),
    defaultLocale,
    deployAppApiBaseUrl: readAbsoluteHttpUrl(
      value[keys.deployAppApiBaseUrl],
      keys.deployAppApiBaseUrl,
      environment,
    ),
    deploymentProfile,
    driveAppApiBaseUrl: readAbsoluteHttpUrl(
      value[keys.driveAppApiBaseUrl],
      keys.driveAppApiBaseUrl,
      environment,
    ),
    environment,
    fallbackLocale,
    profileId: expectedProfileId,
    runtimeTarget: "mini-program",
    supportedLocales,
  };
}

/** Read a base URL out of the emitted runtime module (`module.exports = {...}`). */
export function readWebserverMpRuntimeConfigSource(
  source: unknown,
  runtimeProfile?: WebserverMpDeploymentProfile,
): WebserverMpRuntimeConfig {
  return parseWebserverMpRuntimeConfig(source, runtimeProfile);
}

function readAbsoluteHttpUrl(value: unknown, field: string, environment: string): string {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new Error(`${field} is required`);
  }
  let url: URL;
  try {
    url = new URL(value.trim());
  } catch {
    throw new Error(`${field} must be an absolute HTTP(S) URL`);
  }
  if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) {
    throw new Error(`${field} must be an absolute HTTP(S) URL`);
  }
  if (environment === "production" && ["localhost", "127.0.0.1", "::1"].includes(url.hostname)) {
    throw new Error(`${field} cannot use a loopback host in production`);
  }
  return url.origin;
}

function readLocales(value: unknown, field: string): WebserverMpLocale[] {
  const candidates = typeof value === "string"
    ? value.split(",").map((entry) => entry.trim()).filter((entry) => entry.length > 0)
    : Array.isArray(value)
      ? value
      : [];
  if (candidates.length === 0) {
    throw new Error(`${field} is required`);
  }
  return [...new Set(candidates.map((locale) => readEnum(locale, LOCALES, field)))];
}

function readEnum<const T extends readonly string[]>(
  value: unknown,
  allowed: T,
  field: string,
): T[number] {
  if (typeof value !== "string" || !allowed.includes(value)) {
    throw new Error(`${field} is invalid`);
  }
  return value as T[number];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
