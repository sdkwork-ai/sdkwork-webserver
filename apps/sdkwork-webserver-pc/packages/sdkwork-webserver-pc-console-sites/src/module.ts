import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "applications",
  label: "applications",
  surface: "app-console",
  entries: [
    { resource: "applications", label: "Applications", description: "Application lifecycle and availability", permission: "web.applications.read", order: 1 }
  ],
} as const satisfies WebserverPcModuleDefinition;
