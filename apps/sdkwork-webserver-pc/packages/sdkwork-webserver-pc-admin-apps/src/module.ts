import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "apps",
  label: "apps",
  surface: "backend-admin",
  entries: [
    { resource: "apps", label: "Applications", description: "Publish and operate deploy_app applications", permission: "deploy.apps.read", order: 1 }
  ],
} as const satisfies WebserverPcModuleDefinition;
