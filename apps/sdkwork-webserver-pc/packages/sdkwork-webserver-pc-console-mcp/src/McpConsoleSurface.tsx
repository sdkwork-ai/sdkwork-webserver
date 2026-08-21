import type { AuthTokenManager } from "@sdkwork/sdk-common";
import { createMCPClients, MCPClientsProvider } from "@sdkwork/mcp-pc-core";
import {
  EditMcpServerPage,
  MyMcpServersPage,
  RegisterMcpServerPage,
} from "@sdkwork/mcp-pc-console-mcp";
import { useMemo } from "react";
import { Route, Routes } from "react-router-dom";

/**
 * Bridges the SDKWork MCP self-service console into the Web Server console.
 * The menu entry (My MCP Servers) stays in the host while the pages are the
 * canonical sdkwork-mcp implementation, sharing the IAM dual-token session
 * through the injected token manager. Styles are scoped by
 * `.mcp-console-surface`.
 */
export interface McpConsoleSurfaceProps {
  appApiBaseUrl: string;
  backendApiBaseUrl: string;
  driveAppApiBaseUrl: string;
  resource: "mcp";
  tokenManager: AuthTokenManager;
}

export function McpConsoleSurface({
  appApiBaseUrl,
  backendApiBaseUrl,
  driveAppApiBaseUrl,
  tokenManager,
}: McpConsoleSurfaceProps) {
  const clients = useMemo(
    () => createMCPClients({ appApiBaseUrl, backendApiBaseUrl, driveAppApiBaseUrl, tokenManager }),
    [appApiBaseUrl, backendApiBaseUrl, driveAppApiBaseUrl, tokenManager],
  );
  return (
    <div className="mcp-console-surface">
      <MCPClientsProvider clients={clients}>
        <Routes>
          <Route path="" element={<MyMcpServersPage />} />
          <Route path="mine" element={<MyMcpServersPage />} />
          <Route path="register" element={<RegisterMcpServerPage />} />
          <Route path="edit/:serverKey" element={<EditMcpServerPage />} />
        </Routes>
      </MCPClientsProvider>
    </div>
  );
}
