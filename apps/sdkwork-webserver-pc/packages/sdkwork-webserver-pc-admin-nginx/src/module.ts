import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "nginx",
  label: "nginx",
  surface: "backend-admin",
  entries: [
    { resource: "nginx", label: "Nginx", description: "Validate, deploy and reload Nginx configuration", permission: "web.nginx.write", order: 1 }
  ],
} as const satisfies WebserverPcModuleDefinition;
