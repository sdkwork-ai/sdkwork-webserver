import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  MyPluginsPage,
  PluginCategoriesAdminPage,
  PluginsLocaleProvider,
} from "@sdkwork/webserver-pc-console-plugins";
import { createDriveAppClient } from "@sdkwork/webserver-pc-admin-core";
import { useMemo } from "react";

/** Compatible with IAM session-auth boundary attachment (dual-token clients). */
type AttachSdkClientBoundaries = (
  clients: readonly { http?: unknown }[],
) => readonly { http?: unknown }[];

/**
 * Bridges Plugins admin into the Web Server backend-admin console.
 *
 * `resource` selects the page: `plugins` is the cross-user review table,
 * `plugin-categories` is the platform category curator every user's create
 * form reads from. Styles scoped by `.plugins-admin-surface`.
 */
export interface PluginsAdminSurfaceProps {
  attachSdkClientBoundaries?: AttachSdkClientBoundaries;
  driveAppApiBaseUrl: string;
  locale?: string | null;
  /** IAM subject of the operator; gates which rows offer edit/delete. */
  ownerKey: string;
  resource: "plugins" | "plugin-categories";
  tokenManager: AuthTokenManager;
}

export function PluginsAdminSurface({
  attachSdkClientBoundaries,
  driveAppApiBaseUrl,
  locale,
  ownerKey,
  resource,
  tokenManager,
}: PluginsAdminSurfaceProps) {
  const drive = useMemo(() => {
    const next = createDriveAppClient({
      baseUrl: driveAppApiBaseUrl,
      authMode: "dual-token",
      platform: "pc",
      tokenManager,
    });
    attachSdkClientBoundaries?.([next]);
    return next;
  }, [attachSdkClientBoundaries, driveAppApiBaseUrl, tokenManager]);
  const localeKey = locale?.trim() || "en-US";
  return (
    <div className="plugins-admin-surface" lang={localeKey}>
      <PluginsLocaleProvider key={localeKey} locale={locale}>
        {resource === "plugin-categories" ? (
          <PluginCategoriesAdminPage ownerKey={ownerKey} />
        ) : (
          <MyPluginsPage drive={drive} ownerKey={ownerKey} variant="admin" />
        )}
      </PluginsLocaleProvider>
    </div>
  );
}
