import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "servers-explorer",
  label: "servers explorer",
  surface: "backend-admin",
  entries: [
    { resource: "servers-explorer", label: "Server Files", description: "Browse, classify, and operate server deployment projects and files", permission: "web.servers.files.read", order: 1 }
  ],
} as const satisfies WebserverPcModuleDefinition;
