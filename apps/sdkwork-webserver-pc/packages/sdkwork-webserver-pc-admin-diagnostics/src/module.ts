import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "diagnostics",
  label: "diagnostics",
  surface: "backend-admin",
  entries: [
    { resource: "diagnostics", label: "Diagnostics", description: "Runtime status and convergence diagnostics", permission: "web.servers.read", order: 1 }
  ],
} as const satisfies WebserverPcModuleDefinition;
