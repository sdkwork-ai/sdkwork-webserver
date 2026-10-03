import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "app-templates",
  label: "app templates",
  surface: "backend-admin",
  entries: [
    { resource: "template-categories", label: "Template Categories", description: "Marketplace taxonomy for app templates", permission: "deploy.templateCategories.read", order: 1 },
    { resource: "app-templates", label: "App Templates", description: "Author-published listings and moderation", permission: "deploy.appTemplates.read", order: 2 },
    { resource: "app-template-versions", label: "Template Versions", description: "Published version snapshots of one listing", permission: "deploy.appTemplateVersions.read", order: 3 }
  ],
} as const satisfies WebserverPcModuleDefinition;
