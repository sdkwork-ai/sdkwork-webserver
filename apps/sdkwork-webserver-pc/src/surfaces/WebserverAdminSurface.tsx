import { createWebserverAdminRegistry, createWebserverAdminSdkClient, WebserverAdminSdkProvider } from "@sdkwork/webserver-pc-admin-core";
import { WebserverAdminShell } from "@sdkwork/webserver-pc-admin-shell";
import type { WebserverLocale, WebserverPcModuleDefinition, WebserverResourceKey } from "@sdkwork/webserver-pc-commons";
import type { AuthTokenManager } from "@sdkwork/sdk-common";
import type { ReactNode } from "react";
import { useMemo } from "react";

export interface WebserverAdminSurfaceProps {
  backendApiBaseUrl: string;
  locale: WebserverLocale;
  modules: readonly WebserverPcModuleDefinition[];
  onSignOut(): void;
  permissionScope: readonly string[];
  resourceRenderers?: Partial<Record<WebserverResourceKey, ReactNode>>;
  tokenManager: AuthTokenManager;
  userLabel?: string;
}

/**
 * Backend-admin host surface. It owns the admin registry for the resources that
 * still render themselves from it (nginx, servers, diagnostics, audit); every
 * capability bridged from another module — Applications from sdkwork-deployments,
 * Plugins / Skills / MCP / Storage Center from their owning packages — arrives
 * pre-built through `resourceRenderers`. No application lifecycle is
 * implemented on this surface any more.
 */
export function WebserverAdminSurface({ backendApiBaseUrl, locale, modules, onSignOut, permissionScope, resourceRenderers, tokenManager, userLabel }: WebserverAdminSurfaceProps) {
  const client = useMemo(() => createWebserverAdminSdkClient(backendApiBaseUrl, tokenManager), [backendApiBaseUrl, tokenManager]);
  const registry = useMemo(() => createWebserverAdminRegistry(client), [client]);
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
