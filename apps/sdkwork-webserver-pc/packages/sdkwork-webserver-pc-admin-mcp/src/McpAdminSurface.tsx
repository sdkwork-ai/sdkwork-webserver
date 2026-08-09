import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  AdminCategoriesPage,
  AdminInvocationsPage,
  AdminServerDetailPage,
  AdminServersPage,
  McpAdminRouteProvider,
} from "@sdkwork/mcp-pc-admin";
import { createMCPClients, MCPClientsProvider } from "@sdkwork/mcp-pc-core";
import { useMemo } from "react";
import { Link, Route, Routes } from "react-router-dom";

/**
 * Bridges the SDKWork MCP admin surface into the Web Server backend-admin
 * console. The menu entry (MCP Admin) stays in the host while the pages are
 * the canonical sdkwork-mcp implementation, sharing the IAM dual-token
 * session through the injected token manager. The module pages resolve their
 * server list/detail links from {@link McpAdminRouteProvider}; the module's
 * own PC root defaults to `/admin/servers`, this host remounts them at
 * `/admin/mcp/servers`. Styles are scoped by `.mcp-admin-surface`.
 */
export interface McpAdminSurfaceProps {
  appApiBaseUrl: string;
  backendApiBaseUrl: string;
  driveAppApiBaseUrl: string;
  resource: "mcp";
  tokenManager: AuthTokenManager;
}

export function McpAdminSurface({
  appApiBaseUrl,
  backendApiBaseUrl,
  driveAppApiBaseUrl,
  tokenManager,
}: McpAdminSurfaceProps) {
  const clients = useMemo(
    () => createMCPClients({ appApiBaseUrl, backendApiBaseUrl, driveAppApiBaseUrl, tokenManager }),
    [appApiBaseUrl, backendApiBaseUrl, driveAppApiBaseUrl, tokenManager],
  );
  return (
    <div className="mcp-admin-surface">
      <MCPClientsProvider clients={clients}>
        <McpAdminRouteProvider serversBasePath="/admin/mcp/servers">
          <nav className="mb-4 flex gap-3 text-sm font-medium">
            <Link to="/admin/mcp" className="text-blue-600 hover:text-blue-700">
              Servers
            </Link>
            <Link to="/admin/mcp/categories" className="text-blue-600 hover:text-blue-700">
              Categories
            </Link>
            <Link to="/admin/mcp/invocations" className="text-blue-600 hover:text-blue-700">
              Invocations
            </Link>
          </nav>
          <Routes>
            <Route path="" element={<AdminServersPage />} />
            <Route path="servers" element={<AdminServersPage />} />
            <Route path="servers/:serverKey" element={<AdminServerDetailPage />} />
            <Route path="categories" element={<AdminCategoriesPage />} />
            <Route path="invocations" element={<AdminInvocationsPage />} />
          </Routes>
        </McpAdminRouteProvider>
      </MCPClientsProvider>
    </div>
  );
}
