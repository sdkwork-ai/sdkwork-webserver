import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "site-configuration",
  label: "site configuration",
  surface: "app-console",
  entries: [
    { resource: "configuration", label: "Configuration", description: "Environment variables and health checks", permission: "web.applications.write", order: 1 }
  ],
} as const satisfies WebserverPcModuleDefinition;
