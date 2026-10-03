import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "marketplace",
  label: "marketplace",
  surface: "app-console",
  entries: [
    { resource: "marketplace", label: "Marketplace", description: "Browse and acquire published app templates", permission: "deploy.marketplaceTemplates.read", order: 1 },
    { resource: "my-templates", label: "My Templates", description: "Publish your apps as templates and manage versions", permission: "deploy.appTemplates.read", order: 2 }
  ],
} as const satisfies WebserverPcModuleDefinition;
