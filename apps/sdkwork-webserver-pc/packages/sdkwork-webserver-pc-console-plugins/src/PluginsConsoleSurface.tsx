import type { AuthTokenManager } from "@sdkwork/sdk-common";
import { createDriveAppClient } from "@sdkwork/webserver-pc-console-core";
import { useMemo } from "react";
import { PluginsLocaleProvider } from "./locale.tsx";
import { MyPluginsPage } from "./MyPluginsPage.tsx";

/** Compatible with IAM session-auth boundary attachment (dual-token clients). */
type AttachSdkClientBoundaries = (
  clients: readonly { http?: unknown }[],
) => readonly { http?: unknown }[];

/**
 * Bridges the Plugins self-service console into the Web Server console.
 * The catalog persists browser-locally under a per-owner storage key
 * (`sdkwork.webserver.plugins.catalog.v2.<ownerKey>`), so every IAM subject
 * keeps an isolated plugin CRUD space; archives upload through Drive.
 * Platform categories are admin-owned and shared across owners.
 * Styles are scoped by `.plugins-console-surface`.
 */
export interface PluginsConsoleSurfaceProps {
  attachSdkClientBoundaries?: AttachSdkClientBoundaries;
  driveAppApiBaseUrl: string;
  locale?: string | null;
  /**
   * IAM subject owning this session. The plugin catalog is read and written
   * under it, so each user gets an isolated plugin CRUD space in the browser.
   */
  ownerKey: string;
  resource: "plugins";
  tokenManager: AuthTokenManager;
}

export function PluginsConsoleSurface({
  attachSdkClientBoundaries,
  driveAppApiBaseUrl,
  locale,
  ownerKey,
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
        <MyPluginsPage drive={drive} ownerKey={ownerKey} variant="console" />
      </PluginsLocaleProvider>
    </div>
  );
}
