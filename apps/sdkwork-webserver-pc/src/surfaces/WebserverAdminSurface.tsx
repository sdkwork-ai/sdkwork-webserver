import { createWebserverAdminRegistry, createWebserverAdminSdkClient, WebserverAdminSdkProvider } from "@sdkwork/webserver-pc-admin-core";
import { createWebserverAdminApplicationRegistry } from "@sdkwork/webserver-pc-admin-applications";
import { WebserverAdminShell } from "@sdkwork/webserver-pc-admin-shell";
import type { ApplicationMediaStorage, ApplicationSourceStorage, WebserverLocale, WebserverPcModuleDefinition, WebserverResourceKey } from "@sdkwork/webserver-pc-commons";
import type { AuthTokenManager } from "@sdkwork/sdk-common";
import type { ReactNode } from "react";
import { useMemo } from "react";

export interface WebserverAdminSurfaceProps {
  backendApiBaseUrl: string;
  locale: WebserverLocale;
  mediaStorage: ApplicationMediaStorage;
  modules: readonly WebserverPcModuleDefinition[];
  onSignOut(): void;
  permissionScope: readonly string[];
  resourceRenderers?: Partial<Record<WebserverResourceKey, ReactNode>>;
  sourceStorage: ApplicationSourceStorage;
  tokenManager: AuthTokenManager;
  userLabel?: string;
}

export function WebserverAdminSurface({ backendApiBaseUrl, locale, mediaStorage, modules, onSignOut, permissionScope, resourceRenderers, sourceStorage, tokenManager, userLabel }: WebserverAdminSurfaceProps) {
  const client = useMemo(() => createWebserverAdminSdkClient(backendApiBaseUrl, tokenManager), [backendApiBaseUrl, tokenManager]);
  const registry = useMemo(() => ({
    ...createWebserverAdminRegistry(client),
    ...createWebserverAdminApplicationRegistry(client, sourceStorage, mediaStorage),
  }), [client, mediaStorage, sourceStorage]);
  return (
    <WebserverAdminSdkProvider client={client}>
      <WebserverAdminShell
        locale={locale}
        modules={modules}
        onSignOut={onSignOut}
        permissionScope={permissionScope}
        registry={registry}
        resourceRenderers={resourceRenderers}
        userLabel={userLabel}
      />
    </WebserverAdminSdkProvider>
  );
}
