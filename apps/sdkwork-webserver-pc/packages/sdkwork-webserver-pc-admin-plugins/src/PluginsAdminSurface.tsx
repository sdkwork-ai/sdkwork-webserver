import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  MyPluginsPage,
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
 * Shares the console catalog model; styles scoped by `.plugins-admin-surface`.
 */
export interface PluginsAdminSurfaceProps {
  attachSdkClientBoundaries?: AttachSdkClientBoundaries;
  driveAppApiBaseUrl: string;
  locale?: string | null;
  resource: "plugins";
  tokenManager: AuthTokenManager;
}

export function PluginsAdminSurface({
  attachSdkClientBoundaries,
  driveAppApiBaseUrl,
  locale,
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
        <MyPluginsPage drive={drive} variant="admin" />
      </PluginsLocaleProvider>
    </div>
  );
}
