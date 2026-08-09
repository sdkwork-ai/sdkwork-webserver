import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "skills",
  label: "skills",
  surface: "backend-admin",
  entries: [
    { resource: "skills", label: "Skills Admin", description: "Manage skill packages, categories, and capabilities", permission: "skills.packages.manage", order: 1 }
  ],
} as const satisfies WebserverPcModuleDefinition;
