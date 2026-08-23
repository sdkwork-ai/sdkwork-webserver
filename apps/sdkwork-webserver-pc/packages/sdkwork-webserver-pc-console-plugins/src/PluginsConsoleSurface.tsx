import type { AuthTokenManager } from "@sdkwork/sdk-common";
import { createDriveAppClient } from "@sdkwork/drive-app-sdk";
import { useMemo } from "react";
import { PluginsLocaleProvider } from "./locale.tsx";
import { MyPluginsPage } from "./MyPluginsPage.tsx";

/** Compatible with IAM session-auth boundary attachment (dual-token clients). */
type AttachSdkClientBoundaries = (
  clients: readonly { http?: unknown }[],
) => readonly { http?: unknown }[];

/**
 * Bridges the Plugins self-service console into the Web Server console.
 * Catalog is browser-local for this phase; archives upload through Drive.
 * Styles are scoped by `.plugins-console-surface`.
 */
export interface PluginsConsoleSurfaceProps {
  attachSdkClientBoundaries?: AttachSdkClientBoundaries;
  driveAppApiBaseUrl: string;
  locale?: string | null;
  resource: "plugins";
  tokenManager: AuthTokenManager;
}

export function PluginsConsoleSurface({
  attachSdkClientBoundaries,
  driveAppApiBaseUrl,
  locale,
  tokenManager,
}: PluginsConsoleSurfaceProps) {
  const drive = useMemo(() => {
    const next = createDriveAppClient({
      baseUrl: driveAppApiBaseUrl,
      authMode: "dual-token",
      platform: "pc",
      tokenManager,
    });
    // Dual-token only — never project x-sdkwork-tenant-id (API_SPEC §10.2).
    attachSdkClientBoundaries?.([next]);
    return next;
  }, [attachSdkClientBoundaries, driveAppApiBaseUrl, tokenManager]);
  const localeKey = locale?.trim() || "en-US";
  return (
    <div className="plugins-console-surface" lang={localeKey}>
      <PluginsLocaleProvider key={localeKey} locale={locale}>
        <MyPluginsPage drive={drive} variant="console" />
      </PluginsLocaleProvider>
    </div>
  );
}
