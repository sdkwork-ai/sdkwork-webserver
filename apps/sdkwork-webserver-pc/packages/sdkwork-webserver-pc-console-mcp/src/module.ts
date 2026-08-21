import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "mcp",
  label: "mcp",
  surface: "app-console",
  entries: [
    {
      resource: "mcp",
      label: "My MCP",
      description: "Register and manage MCP servers you own",
      permission: "mcp.marketplace.read",
      order: 1,
    },
  ],
} as const satisfies WebserverPcModuleDefinition;
