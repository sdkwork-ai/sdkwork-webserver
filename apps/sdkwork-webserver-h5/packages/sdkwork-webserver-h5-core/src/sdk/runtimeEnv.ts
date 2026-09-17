/**
 * Browser runtime configuration for the H5 root.
 *
 * The materialized `public/runtime-env.json` (produced by
 * `scripts/materialize-runtime-env.mjs` from `etc/`) is this deployable root's
 * runtime authority. Values are validated here so a mis-materialized profile
 * fails fast instead of silently degrading to a default host.
 *
 * Base URLs are resolved through `resolveBaseUrlWithAlignProtocol`: the serving
 * edge terminates HTTP and HTTPS on the same host, so an `http://` page must
 * target the `http://` origin and an `https://` page `https://`
 * (ENVIRONMENT_SPEC.md §6.3 protocol adaptation).
 */
export { readRuntimeEnv } from "@sdkwork/sdk-common";
import { resolveBaseUrlWithAlignProtocol } from "@sdkwork/sdk-common";

export type WebserverH5LifecycleEnvironment =
  | "development"
  | "test"
  | "staging"
  | "demo"
  | "production";
export type WebserverH5DeploymentProfile = "standalone" | "cloud";
export type WebserverH5Locale = "en-US" | "zh-CN";

export interface WebserverH5RuntimeConfig {
  activeLocales: WebserverH5Locale[];
  appApiBaseUrl: string;
  defaultLocale: WebserverH5Locale;
  deployAppApiBaseUrl: string;
  deploymentProfile: WebserverH5DeploymentProfile;
  driveAppApiBaseUrl: string;
  environment: WebserverH5LifecycleEnvironment;
  fallbackLocale: WebserverH5Locale;
  profileId: string;
  supportedLocales: WebserverH5Locale[];
}

const ENVIRONMENTS = ["development", "test", "staging", "demo", "production"] as const;
const PROFILES = ["standalone", "cloud"] as const;
const LOCALES = ["en-US", "zh-CN"] as const;

export async function loadWebserverH5RuntimeConfig(
  fetcher: typeof fetch = fetch,
): Promise<WebserverH5RuntimeConfig> {
  const response = await fetcher("/runtime-env.json", {
    cache: "no-store",
    credentials: "same-origin",
  });
  if (!response.ok) {
    throw new Error(`Runtime configuration failed with HTTP ${response.status}`);
  }
  return parseWebserverH5RuntimeConfig(await response.json());
}

export function parseWebserverH5RuntimeConfig(value: unknown): WebserverH5RuntimeConfig {
  if (!isRecord(value)) throw new Error("Runtime configuration must be an object");
  const environment = readEnum(value.environment, ENVIRONMENTS, "environment");
  const deploymentProfile = readEnum(value.deploymentProfile, PROFILES, "deploymentProfile");
  const expectedProfileId = `${deploymentProfile}.${environment}`;
  if (value.profileId !== expectedProfileId) {
    throw new Error(`profileId must equal ${expectedProfileId}`);
  }
  const supportedLocales = readLocales(value.supportedLocales, "supportedLocales");
  const activeLocales = readLocales(value.activeLocales, "activeLocales");
  const defaultLocale = readEnum(value.defaultLocale, LOCALES, "defaultLocale");
  const fallbackLocale = readEnum(value.fallbackLocale, LOCALES, "fallbackLocale");
  if (
    !supportedLocales.includes(defaultLocale)
    || !supportedLocales.includes(fallbackLocale)
    || activeLocales.some((locale) => !supportedLocales.includes(locale))
  ) {
    throw new Error("Locale configuration is inconsistent");
  }
  return {
    activeLocales,
    appApiBaseUrl: resolveWebserverH5BaseUrl(value.appApiBaseUrl, "appApiBaseUrl", deploymentProfile),
    defaultLocale,
    deployAppApiBaseUrl: resolveWebserverH5BaseUrl(
      value.deployAppApiBaseUrl,
      "deployAppApiBaseUrl",
      deploymentProfile,
    ),
    deploymentProfile,
    driveAppApiBaseUrl: resolveWebserverH5BaseUrl(
      value.driveAppApiBaseUrl,
      "driveAppApiBaseUrl",
      deploymentProfile,
    ),
    environment,
    fallbackLocale,
    profileId: expectedProfileId,
    supportedLocales,
  };
}

/**
 * Resolve one SDK base URL. A standalone root declares the canonical
 * same-origin root `/`; a cloud root declares an absolute HTTP(S) URL. Either
 * way the result is an absolute origin with the current page protocol applied.
 */
export function resolveWebserverH5BaseUrl(
  configured: unknown,
  field: string,
  deploymentProfile: WebserverH5DeploymentProfile,
): string {
  if (configured === "/") {
    if (deploymentProfile !== "standalone") {
      throw new Error(`${field} must be an absolute URL for the cloud profile`);
    }
  } else {
    readAbsoluteHttpUrl(configured, field);
  }
  const resolution = resolveBaseUrlWithAlignProtocol({
    baseUrls: [String(configured)],
    mode: deploymentProfile,
  });
  if (!resolution.url) {
    throw new Error(`${field} could not be resolved to an absolute origin`);
  }
  return resolution.url;
}

function readAbsoluteHttpUrl(value: unknown, field: string): string {
  if (typeof value !== "string" || !value.trim()) throw new Error(`${field} is required`);
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new Error(`${field} must be an absolute HTTP(S) URL`);
  }
  if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) {
    throw new Error(`${field} must be an absolute HTTP(S) URL`);
  }
  return url.toString().replace(/\/$/, "");
}

function readLocales(value: unknown, field: string): WebserverH5Locale[] {
  if (!Array.isArray(value) || value.length === 0) throw new Error(`${field} is required`);
  return [...new Set(value.map((locale) => readEnum(locale, LOCALES, field)))];
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
