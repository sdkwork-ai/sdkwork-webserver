import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "webserver-config",
  label: "webserver config",
  surface: "backend-admin",
  entries: [
    { resource: "webserver-config", label: "Server Config", description: "Edit the deployed default config, import plane, and module sidecar configuration online", permission: "web.servers.files.read", order: 1 }
  ],
} as const satisfies WebserverPcModuleDefinition;
