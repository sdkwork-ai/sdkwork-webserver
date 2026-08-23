import type { AuthTokenManager } from "@sdkwork/sdk-common";
import { createMCPClients, MCPClientsProvider } from "@sdkwork/mcp-pc-core";
import {
  EditMcpServerPage,
  McpConsoleLocaleProvider,
  MyMcpServersPage,
  RegisterMcpServerPage,
} from "@sdkwork/mcp-pc-console-mcp";
import { useMemo } from "react";
import { Route, Routes } from "react-router-dom";

/** Compatible with IAM session-auth boundary attachment (dual-token clients). */
type AttachSdkClientBoundaries = (
  clients: readonly { http?: unknown }[],
) => readonly { http?: unknown }[];

/**
 * Bridges the SDKWork MCP self-service console into the Web Server console.
 * The menu entry (My MCP Servers) stays in the host while the pages are the
 * canonical sdkwork-mcp implementation, sharing the IAM dual-token session
 * through the injected token manager. Styles are scoped by
 * `.mcp-console-surface`.
 */
export interface McpConsoleSurfaceProps {
  appApiBaseUrl: string;
  attachSdkClientBoundaries?: AttachSdkClientBoundaries;
  backendApiBaseUrl: string;
  driveAppApiBaseUrl: string;
  locale?: string | null;
  resource: "mcp";
  tokenManager: AuthTokenManager;
}

export function McpConsoleSurface({
  appApiBaseUrl,
  attachSdkClientBoundaries,
  backendApiBaseUrl,
  driveAppApiBaseUrl,
  locale,
  tokenManager,
}: McpConsoleSurfaceProps) {
  const clients = useMemo(() => {
    const next = createMCPClients({
      appApiBaseUrl,
      backendApiBaseUrl,
      driveAppApiBaseUrl,
      tokenManager,
    });
    // Dual-token only — never project x-sdkwork-tenant-id (API_SPEC §10.2).
    attachSdkClientBoundaries?.([next.app, next.backend, next.drive]);
    return next;
  }, [
    appApiBaseUrl,
    attachSdkClientBoundaries,
    backendApiBaseUrl,
    driveAppApiBaseUrl,
    tokenManager,
  ]);
  const localeKey = locale?.trim() || "en-US";
  return (
    <div className="mcp-console-surface" lang={localeKey}>
      <McpConsoleLocaleProvider key={localeKey} locale={locale}>
        <MCPClientsProvider clients={clients}>
          <Routes>
            <Route path="" element={<MyMcpServersPage />} />
            <Route path="mine" element={<MyMcpServersPage />} />
            <Route path="register" element={<RegisterMcpServerPage />} />
            <Route path="edit/:serverKey" element={<EditMcpServerPage />} />
          </Routes>
        </MCPClientsProvider>
      </McpConsoleLocaleProvider>
    </div>
  );
}
