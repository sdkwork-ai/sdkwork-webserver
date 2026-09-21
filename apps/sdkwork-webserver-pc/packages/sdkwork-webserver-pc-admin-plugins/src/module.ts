import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "plugins",
  label: "plugins",
  surface: "backend-admin",
  entries: [
    {
      resource: "plugins",
      label: "Plugins Admin",
      description: "Review and maintain registered workspace plugins",
      permission: "skills.packages.manage",
      order: 40,
    },
    {
      resource: "plugin-categories",
      label: "Plugin Categories",
      description: "Curate the platform categories users file plugins under",
      permission: "skills.packages.manage",
      order: 41,
    },
  ],
} as const satisfies WebserverPcModuleDefinition;
