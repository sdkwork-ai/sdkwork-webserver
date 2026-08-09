import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "mcp",
  label: "mcp",
  surface: "backend-admin",
  entries: [
    { resource: "mcp", label: "MCP Admin", description: "Manage MCP servers, categories, and invocations", permission: "mcp.admin.server.manage", order: 1 }
  ],
} as const satisfies WebserverPcModuleDefinition;
