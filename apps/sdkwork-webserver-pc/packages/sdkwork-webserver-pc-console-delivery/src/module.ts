import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "delivery",
  label: "delivery",
  surface: "app-console",
  entries: [
    { resource: "domains", label: "Domains", description: "Domain ownership and routing", permission: "web.applications.write", order: 1 },
    { resource: "certificates", label: "Certificates", description: "TLS certificate lifecycle", permission: "web.certificates.read", order: 2 }
  ],
} as const satisfies WebserverPcModuleDefinition;
