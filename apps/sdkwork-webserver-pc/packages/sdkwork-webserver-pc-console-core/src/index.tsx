import { createDriveAppClient, type SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";
import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  createClient as createWebAppClient,
  type SdkworkAppClient as SdkworkWebAppClient,
} from "@sdkwork/webserver-app-sdk";
import { createContext, useContext, type ReactNode } from "react";

/**
 * Console SDK wiring. This package owns exactly one thing: turning the host's
 * base URLs plus token manager into the two generated clients the console
 * surfaces consume, and publishing them through a typed host port.
 *
 * The application lifecycle used to live here as a resource registry over
 * `web_application` / `web_source_version` / `web_deployment`. That entity is
 * owned by sdkwork-deployments (`deploy_app`) and every Applications page is now
 * the canonical deployments page bridged by
 * `@sdkwork/webserver-pc-console-delivery`, so no registry-driven console
 * resource remains and nothing local re-implements the lifecycle.
 */

export type { SdkworkDriveAppClient };
export { createDriveAppClient };

export type WebserverConsoleSdkClient = SdkworkWebAppClient;

export interface WebserverConsoleSdkClients {
  drive: SdkworkDriveAppClient;
  web: SdkworkWebAppClient;
}

const Context = createContext<WebserverConsoleSdkClients | null>(null);

export function createWebserverConsoleSdkClient(baseUrl: string, tokenManager: AuthTokenManager): WebserverConsoleSdkClient { return createWebAppClient({ baseUrl, authMode: "dual-token", platform: "pc", tokenManager }); }
export function createWebserverConsoleSdkClients(baseUrls: { driveAppApiBaseUrl: string; webAppApiBaseUrl: string }, tokenManager: AuthTokenManager): WebserverConsoleSdkClients { return { drive: createDriveAppClient({ baseUrl: baseUrls.driveAppApiBaseUrl, authMode: "dual-token", platform: "pc", tokenManager }), web: createWebserverConsoleSdkClient(baseUrls.webAppApiBaseUrl, tokenManager) }; }
export function WebserverConsoleSdkProvider({ children, clients }: { children: ReactNode; clients: WebserverConsoleSdkClients }) { return <Context.Provider value={clients}>{children}</Context.Provider>; }
export function useWebserverConsoleSdk(): WebserverConsoleSdkClients { const clients = useContext(Context); if (!clients) throw new Error("WebserverConsoleSdkProvider is required"); return clients; }
