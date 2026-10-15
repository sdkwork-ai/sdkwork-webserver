import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "servers",
  label: "servers",
  surface: "backend-admin",
  entries: [
    { resource: "servers", label: "Servers", description: "Managed Web Server inventory", permission: "web.servers.read", order: 1 }
  ],
} as const satisfies WebserverPcModuleDefinition;
