import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "delivery",
  label: "delivery",
  surface: "backend-admin",
  entries: [
    { resource: "domains", label: "Domains", description: "Root domains and subdomains this edge serves, reconciled from its configuration", permission: "web.sites.read", order: 1 },
    { resource: "certificates", label: "Certificates", description: "TLS certificate lifecycle for the served hostnames", permission: "web.certificates.read", order: 2 }
  ],
} as const satisfies WebserverPcModuleDefinition;
