export type WebserverLifecycleEnvironment = "development" | "test" | "staging" | "demo" | "production";
export type WebserverDeploymentProfile = "standalone" | "cloud";
export type WebserverBrowserOriginMode = "same-origin" | "cross-origin";
export type WebserverLocale = "en-US" | "zh-CN";

export interface WebserverPcRuntimeConfig {
  activeLocales: WebserverLocale[];
  appApiBaseUrl: string;
  appbaseAppApiBaseUrl: string;
  backendApiBaseUrl: string;
  browserOriginMode: WebserverBrowserOriginMode;
  deployAppApiBaseUrl: string;
  driveAppApiBaseUrl: string;
  defaultLocale: WebserverLocale;
  deploymentProfile: WebserverDeploymentProfile;
  environment: WebserverLifecycleEnvironment;
  fallbackLocale: WebserverLocale;
  messagingPcUrl: string;
  profileId: `${WebserverDeploymentProfile}.${WebserverLifecycleEnvironment}`;
  runtimeTarget: "browser";
  supportedLocales: WebserverLocale[];
}

export async function loadWebserverPcRuntimeConfig(
  fetcher: typeof fetch = fetch,
  browserOrigin: string | undefined = currentBrowserOrigin(),
): Promise<WebserverPcRuntimeConfig> {
  const response = await fetcher("/runtime-env.json", { cache: "no-store", credentials: "same-origin" });
  if (!response.ok) throw new Error(`Runtime configuration failed with HTTP ${response.status}`);
  return parseWebserverPcRuntimeConfig(await response.json(), browserOrigin);
}

export function parseWebserverPcRuntimeConfig(
  value: unknown,
  browserOrigin?: string,
): WebserverPcRuntimeConfig {
  if (!isRecord(value)) throw new Error("Runtime configuration must be an object");
  const environment = readEnum(value.environment, ["development", "test", "staging", "demo", "production"] as const, "environment");
  const deploymentProfile = readEnum(value.deploymentProfile, ["standalone", "cloud"] as const, "deploymentProfile");
  const profileId = readProfileId(value.profileId, deploymentProfile, environment);
  const runtimeTarget = readEnum(value.runtimeTarget, ["browser"] as const, "runtimeTarget");
  const browserOriginMode = readEnum(value.browserOriginMode, ["same-origin", "cross-origin"] as const, "browserOriginMode");
  const supportedLocales = readLocales(value.supportedLocales, "supportedLocales");
  const activeLocales = readLocales(value.activeLocales, "activeLocales");
  const defaultLocale = readEnum(value.defaultLocale, ["en-US", "zh-CN"] as const, "defaultLocale");
  const fallbackLocale = readEnum(value.fallbackLocale, ["en-US", "zh-CN"] as const, "fallbackLocale");
  if (!supportedLocales.includes(defaultLocale) || !supportedLocales.includes(fallbackLocale) || activeLocales.some((locale) => !supportedLocales.includes(locale))) throw new Error("Locale configuration is inconsistent");
  const baseUrls = deploymentProfile === "standalone"
    ? readStandaloneBaseUrls(value, browserOrigin, browserOriginMode, environment)
    : readCloudBaseUrls(value, environment, browserOriginMode);
  const messagingPcUrl = readUrl(value.messagingPcUrl, "messagingPcUrl", environment);
  return { activeLocales, ...baseUrls, browserOriginMode, defaultLocale, deploymentProfile, environment, fallbackLocale, messagingPcUrl, profileId, runtimeTarget, supportedLocales };
}

export function resolveWebserverLocale(config: WebserverPcRuntimeConfig, preferredLocales: readonly string[]): WebserverLocale {
  for (const preferred of preferredLocales) {
    const normalized = preferred.toLowerCase().startsWith("zh") ? "zh-CN" : preferred.toLowerCase().startsWith("en") ? "en-US" : undefined;
    if (normalized && config.activeLocales.includes(normalized)) return normalized;
  }
  return config.activeLocales.includes(config.defaultLocale) ? config.defaultLocale : config.fallbackLocale;
}

function readStandaloneBaseUrls(value: Record<string, unknown>, browserOrigin: string | undefined, browserOriginMode: WebserverBrowserOriginMode, environment: WebserverLifecycleEnvironment) {
  if (browserOriginMode !== "same-origin") throw new Error("standalone browserOriginMode must equal same-origin");
  const origin = readBrowserOrigin(browserOrigin);
  for (const field of ["appApiBaseUrl", "backendApiBaseUrl", "driveAppApiBaseUrl", "appbaseAppApiBaseUrl"] as const) {
    if (value[field] !== "/") throw new Error(`${field} must use the canonical standalone same-origin root /`);
  }
  return { appApiBaseUrl: origin, appbaseAppApiBaseUrl: origin, backendApiBaseUrl: origin, deployAppApiBaseUrl: readDeployBaseUrl(value, origin, environment), driveAppApiBaseUrl: origin };
}
function readCloudBaseUrls(value: Record<string, unknown>, environment: WebserverLifecycleEnvironment, browserOriginMode: WebserverBrowserOriginMode) {
  if (browserOriginMode !== "cross-origin") throw new Error("cloud browserOriginMode must equal cross-origin");
  return {
    appApiBaseUrl: readUrl(value.appApiBaseUrl, "appApiBaseUrl", environment),
    appbaseAppApiBaseUrl: readUrl(value.appbaseAppApiBaseUrl, "appbaseAppApiBaseUrl", environment),
    backendApiBaseUrl: readUrl(value.backendApiBaseUrl, "backendApiBaseUrl", environment),
    deployAppApiBaseUrl: readUrl(value.deployAppApiBaseUrl, "deployAppApiBaseUrl", environment),
    driveAppApiBaseUrl: readUrl(value.driveAppApiBaseUrl, "driveAppApiBaseUrl", environment),
  };
}
function readDeployBaseUrl(value: Record<string, unknown>, fallbackOrigin: string, environment: WebserverLifecycleEnvironment): string {
  const raw = value.deployAppApiBaseUrl;
  if (raw === undefined || raw === "/") return fallbackOrigin;
  return readUrl(raw, "deployAppApiBaseUrl", environment);
}
function readBrowserOrigin(value: string | undefined): string { if (typeof value !== "string" || !value.trim()) throw new Error("browser origin is required for standalone runtime config"); let url: URL; try { url = new URL(value); } catch { throw new Error("browser origin must be an absolute HTTP(S) origin"); } if (!["http:", "https:"].includes(url.protocol) || url.username || url.password || url.pathname !== "/" || url.search || url.hash) throw new Error("browser origin must be an absolute HTTP(S) origin"); return url.origin; }
function readUrl(value: unknown, field: string, environment: WebserverLifecycleEnvironment): string { if (typeof value !== "string" || !value.trim()) throw new Error(`${field} is required`); let url: URL; try { url = new URL(value); } catch { throw new Error(`${field} must be an absolute HTTP(S) URL`); } if (!["http:", "https:"].includes(url.protocol) || url.username || url.password) throw new Error(`${field} must be an absolute HTTP(S) URL`); if (environment === "production" && ["localhost", "127.0.0.1", "::1"].includes(url.hostname)) throw new Error(`${field} cannot use a loopback host in production`); return url.toString().replace(/\/$/, ""); }
function readProfileId(value: unknown, deploymentProfile: WebserverDeploymentProfile, environment: WebserverLifecycleEnvironment): `${WebserverDeploymentProfile}.${WebserverLifecycleEnvironment}` { const expected = `${deploymentProfile}.${environment}` as const; if (value !== expected) throw new Error(`profileId must equal ${expected}`); return expected; }
function readLocales(value: unknown, field: string): WebserverLocale[] { if (!Array.isArray(value) || value.length === 0) throw new Error(`${field} is required`); return [...new Set(value.map((locale) => readEnum(locale, ["en-US", "zh-CN"] as const, field)))]; }
function readEnum<const T extends readonly string[]>(value: unknown, allowed: T, field: string): T[number] { if (typeof value !== "string" || !allowed.includes(value)) throw new Error(`${field} is invalid`); return value as T[number]; }
function isRecord(value: unknown): value is Record<string, unknown> { return typeof value === "object" && value !== null && !Array.isArray(value); }
function currentBrowserOrigin(): string | undefined { return typeof window === "undefined" ? undefined : window.location.origin; }
