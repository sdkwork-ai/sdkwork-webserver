/**
 * The `deploy_app` SDK **port** for the Web Server HarmonyOS root.
 *
 * Authority: `HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §6 and
 * `APP_SDK_INTEGRATION_SPEC.md` §2.
 *
 * The `deploy_app` entity is owned by `sdkwork-deployments`, so this root
 * reaches it through a port named for that authority rather than through a
 * second, webserver-owned application surface
 * (`COMPOSABLE_ARCHITECTURE_SPEC.md` §7: one owner per normalized route). This
 * file owns the injected port contract, the base-URL normalization, the closed
 * value sets the UI maps onto, and the credential resolution boundary. Concrete
 * transport construction stays in
 * `entry/src/main/ets/bootstrap/SdkClients.ets`.
 *
 * ## Prerequisite (declared platform gap, not a silent fallback)
 *
 * `sdkwork-deployments-app-sdk` currently generates **no ArkTS target** — its
 * only materialized variant is TypeScript — so no generated ArkTS client exists
 * for this port to bind. This module therefore declares the port plus the
 * adapter seam a future generated ArkTS client must satisfy, and refuses to
 * fabricate a vendored transport copy.
 *
 * A port constructed without a transport reports `available === false` and its
 * readers reject with `WebserverDeployAppSdkUnavailableError`. That is
 * deliberate: a port that resolved to an empty list would render "this tenant
 * has no applications", which is a *lie* about server state, whereas an
 * unavailable transport renders the error state the screen already owns.
 * Feature packages must never fill this gap with raw request APIs or manual auth
 * headers.
 */

export const WEBSERVER_APP_API_SUFFIX: string = '/app/v3/api';

/**
 * Closed `AppKind` / `AppStatus` sets.
 *
 * Mirror of `sdkwork-deployments-app-sdk-typescript/generated/server-openapi`'s
 * `AppKind` and `AppStatus` unions. They are mirrored (not imported) only
 * because no ArkTS target is generated today, and
 * `tests/harmony-surface-contract.test.mjs` re-derives both sets from the
 * generated TypeScript SDK so the mirror cannot drift unnoticed.
 */
export const WEBSERVER_APP_KINDS: string[] = [
  'STATIC_WEB',
  'SPA_WEB',
  'API_SERVICE',
  'WECHAT_MINIPROGRAM',
  'DOUYIN_MINIPROGRAM',
  'IOS_APP',
  'ANDROID_APP',
  'HARMONYOS_APP',
  'DESKTOP_APP',
];

export const WEBSERVER_APP_STATUSES: string[] = [
  'DRAFT',
  'READY',
  'ACTIVE',
  'PAUSED',
  'ARCHIVED',
  'FAILED',
];

export interface WebserverDeployAppSdkClientConfig {
  readonly baseUrl: string;
  readonly accessToken?: string;
  readonly authToken?: string;
  readonly platform?: string;
}

/** One `deploy_app` record projected for the console list. */
export interface WebserverApplicationRecord {
  readonly id: string;
  readonly name: string;
  readonly slug: string;
  readonly appKind: string;
  readonly appStatus: string;
  readonly description: string;
  readonly defaultEnvironment: string;
  readonly latestReleaseTag: string;
  readonly platformTargetCount: number;
  readonly updatedAt: string;
}

export interface WebserverApplicationsPage {
  readonly items: WebserverApplicationRecord[];
  readonly pageInfo: WebserverHarmonyPageInfoLike;
}

/**
 * The single generated operation this root currently consumes, named as a slice
 * rather than as a whole client so a screen cannot reach for a create/update
 * operation it never declared. A generated ArkTS client satisfies it
 * structurally, so the real client passes through with no adapter.
 */
export interface WebserverApplicationsListReader {
  list(page: number, pageSize: number): Promise<WebserverApplicationsPage>;
}

/** The adapter surface a generated ArkTS/TypeScript app SDK must provide. */
export interface WebserverDeployAppSdkTransport {
  readonly platform: string;
  readonly applications: WebserverApplicationsListReader;
}

export interface WebserverDeployAppSdkPort {
  readonly baseUrl: string;
  readonly platform: string;
  /** False while the ArkTS-adapted transport is not generated yet. */
  readonly available: boolean;
  readonly applications: WebserverApplicationsListReader;
  dispose(): void;
}

export interface WebserverHarmonySdkPortConfig extends WebserverDeployAppSdkClientConfig {
  readonly transport?: WebserverDeployAppSdkTransport;
}

/** Stable, user-safe failure vocabulary for an unadapted transport. */
export class WebserverDeployAppSdkUnavailableError extends Error {
  readonly code: string = 'app-sdk-unavailable';

  constructor(operation: string) {
    super(
      `the ArkTS-adapted webserver app SDK transport is not generated yet, so ${operation} cannot be issued`,
    );
  }
}

let configuredBaseUrl: string | null = null;

function stripTrailingSlashes(value: string): string {
  let normalized: string = value.trim();
  while (normalized.endsWith('/')) {
    normalized = normalized.slice(0, normalized.length - 1);
  }
  return normalized;
}

/**
 * Record the surface base URL once, at bootstrap.
 *
 * The generated client appends the surface prefix itself, so the runtime value
 * already carries `/app/v3/api` and must carry it exactly once.
 */
export function configureWebserverDeployAppSdkBaseUrl(baseUrl: string): void {
  const normalized: string = stripTrailingSlashes(baseUrl);
  if (normalized.length === 0) {
    throw new Error('webserver app API base URL is required before SDK bootstrap');
  }
  if (!normalized.endsWith(WEBSERVER_APP_API_SUFFIX)) {
    throw new Error(`webserver app API base URL must end with ${WEBSERVER_APP_API_SUFFIX}`);
  }
  const prefix: string = normalized.slice(0, normalized.length - WEBSERVER_APP_API_SUFFIX.length);
  if (prefix.endsWith(WEBSERVER_APP_API_SUFFIX)) {
    throw new Error(
      `webserver app API base URL must contain ${WEBSERVER_APP_API_SUFFIX} exactly once`,
    );
  }
  configuredBaseUrl = normalized;
}

export function resolveWebserverDeployAppSdkBaseUrl(): string {
  if (configuredBaseUrl === null) {
    throw new Error('webserver app API base URL must be configured before SDK bootstrap');
  }
  return configuredBaseUrl;
}

export function resetWebserverDeployAppSdkBaseUrl(): void {
  configuredBaseUrl = null;
}

/** Reader used while no ArkTS transport exists: fails loudly, never blanks out. */
class UnavailableApplicationsListReader implements WebserverApplicationsListReader {
  list(page: number, pageSize: number): Promise<WebserverApplicationsPage> {
    return Promise.reject(
      new WebserverDeployAppSdkUnavailableError(`applications.list(page=${page}, pageSize=${pageSize})`),
    );
  }
}

class WebserverDeployAppSdkPortImpl implements WebserverDeployAppSdkPort {
  readonly baseUrl: string;
  readonly platform: string;
  readonly available: boolean;
  readonly applications: WebserverApplicationsListReader;
  private transport: WebserverDeployAppSdkTransport | null;

  constructor(
    baseUrl: string,
    platform: string,
    transport: WebserverDeployAppSdkTransport | null,
  ) {
    this.baseUrl = baseUrl;
    this.platform = platform;
    this.transport = transport;
    this.available = transport !== null;
    this.applications = transport === null
      ? new UnavailableApplicationsListReader()
      : transport.applications;
  }

  dispose(): void {
    this.transport = null;
  }
}

let sdkPort: WebserverDeployAppSdkPort | null = null;

/**
 * Build the port from the configured surface URL.
 *
 * Passing `transport` is how a future generated ArkTS client — or a
 * deterministic double in `ohosTest` — is injected. The port is memoized so every
 * capability package shares one credential boundary.
 */
export function createWebserverHarmonySdkPort(
  config: WebserverHarmonySdkPortConfig,
): WebserverDeployAppSdkPort {
  configureWebserverDeployAppSdkBaseUrl(config.baseUrl);
  const platform: string = config.platform ?? 'harmony-native';
  sdkPort = new WebserverDeployAppSdkPortImpl(
    resolveWebserverDeployAppSdkBaseUrl(),
    platform,
    config.transport ?? null,
  );
  return sdkPort;
}

export function getWebserverHarmonySdkPort(): WebserverDeployAppSdkPort {
  if (sdkPort === null) {
    return createWebserverHarmonySdkPort({
      baseUrl: resolveWebserverDeployAppSdkBaseUrl(),
    });
  }
  return sdkPort;
}

export function resetWebserverHarmonySdkPort(): void {
  sdkPort = null;
}
