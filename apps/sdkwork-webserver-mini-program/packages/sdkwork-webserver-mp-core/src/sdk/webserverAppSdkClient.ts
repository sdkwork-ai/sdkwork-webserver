import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  createClient,
  type SdkworkAppClient as GeneratedWebserverAppClient,
  type SdkworkAppConfig,
} from "@sdkwork/webserver-app-sdk";

import { getWebserverMpGlobalTokenManager } from "../session/session";

/**
 * The single construction point for the webserver app SDK client in this root.
 * `APP_SDK_INTEGRATION_SPEC.md` §2 requires generated clients to be created in
 * core and injected into capability packages — capability code never calls
 * `createClient` itself and never builds a raw transport.
 */
export type WebserverMpAppSdkClient = GeneratedWebserverAppClient;

export interface CreateWebserverMpAppSdkClientOptions {
  /** Absolute application origin resolved from the runtime profile. */
  baseUrl: string;
  /** Defaults to the process-wide mini program token manager. */
  tokenManager?: AuthTokenManager;
  /** Request timeout override; deployment profiles pick the default. */
  timeoutMs?: number;
}

export function createWebserverMpAppSdkClient(
  options: CreateWebserverMpAppSdkClientOptions,
): WebserverMpAppSdkClient {
  const config: SdkworkAppConfig = {
    baseUrl: options.baseUrl,
    authMode: "dual-token",
    platform: "mini-program",
    tokenManager: options.tokenManager ?? getWebserverMpGlobalTokenManager(),
    ...(options.timeoutMs ? { timeout: options.timeoutMs } : {}),
  };
  return createClient(config);
}

export type { SdkworkAppConfig };

/**
 * Re-exported so capability packages can name the response types they render
 * without deep-importing generated SDK transport modules.
 */
export type {
  ApplicationResponse,
  CreateApplicationRequest,
  PageInfo,
  UpdateApplicationRequest,
} from "@sdkwork/webserver-app-sdk";
