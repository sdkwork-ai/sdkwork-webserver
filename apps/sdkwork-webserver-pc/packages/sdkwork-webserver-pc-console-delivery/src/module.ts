import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "delivery",
  label: "delivery",
  surface: "app-console",
  entries: [
    { resource: "apps", label: "Applications", description: "Publish and operate deploy_app applications", permission: "deploy.apps.read", order: 1 },
    { resource: "domains", label: "Domains", description: "Domain ownership and routing", permission: "deploy.domainZones.read", order: 2 },
    { resource: "certificates", label: "Certificates", description: "TLS certificate lifecycle", permission: "deploy.certificates.read", order: 3 }
  ],
} as const satisfies WebserverPcModuleDefinition;
