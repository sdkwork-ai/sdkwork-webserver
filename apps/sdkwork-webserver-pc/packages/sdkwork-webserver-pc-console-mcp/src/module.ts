import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "mcp",
  label: "mcp",
  surface: "app-console",
  entries: [
    { resource: "mcp", label: "My MCP Servers", description: "MCP servers registered by the authenticated user", permission: "mcp.marketplace.read", order: 1 }
  ],
} as const satisfies WebserverPcModuleDefinition;
