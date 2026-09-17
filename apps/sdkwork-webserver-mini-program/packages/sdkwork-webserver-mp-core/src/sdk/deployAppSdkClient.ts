import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  createClient,
  type SdkworkAppConfig as DeployAppSdkConfig,
  type SdkworkDeployAppClient as GeneratedDeployAppClient,
} from "@sdkwork/deployments-app-sdk";

import { getWebserverMpGlobalTokenManager } from "../session/session";

/**
 * Deployments app SDK construction.
 *
 * The `deploy_app` application lifecycle is owned by `sdkwork-deployments`, and
 * the PC console mounts the canonical publishing page from
 * `@sdkwork/deployments-pc-console-publishing` rather than re-implementing it.
 * That package is React-PC only, so this native root composes the same
 * **authority** (the deployments app API) by building its own client here in
 * core — which is where `APP_SDK_INTEGRATION_SPEC.md` §2 requires generated
 * clients to be constructed — and injecting it into the applications capability
 * package.
 */
export type WebserverMpDeployAppClient = GeneratedDeployAppClient;

export interface CreateWebserverMpDeployAppSdkClientOptions {
  /** Absolute application origin resolved from the runtime profile. */
  baseUrl: string;
  /** Defaults to the process-wide mini program token manager. */
  tokenManager?: AuthTokenManager;
  /** Request timeout override; deployment profiles pick the default. */
  timeoutMs?: number;
}

export function createWebserverMpDeployAppSdkClient(
  options: CreateWebserverMpDeployAppSdkClientOptions,
): WebserverMpDeployAppClient {
  const config: DeployAppSdkConfig = {
    baseUrl: options.baseUrl,
    authMode: "dual-token",
    platform: "mini-program",
    tokenManager: options.tokenManager ?? getWebserverMpGlobalTokenManager(),
    ...(options.timeoutMs ? { timeout: options.timeoutMs } : {}),
  };
  return createClient(config);
}

export type { DeployAppSdkConfig };

/**
 * Re-exported under `Deploy*` names so capability packages can name the response
 * types they render without deep-importing generated SDK transport modules.
 *
 * The list *parameter* type is deliberately not re-exported: the generated api
 * barrel does not surface it, and capability code builds its query through
 * `toWebserverMpListQuery` instead, which keeps page sizing a core concern.
 */
export type {
  AppKind as DeployAppKind,
  AppResponse as DeployAppResponse,
  AppStatus as DeployAppStatus,
  PageInfo as DeployAppPageInfo,
} from "@sdkwork/deployments-app-sdk";
