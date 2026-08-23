import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "plugins",
  label: "plugins",
  surface: "backend-admin",
  entries: [
    {
      resource: "plugins",
      label: "Plugins Admin",
      description: "Manage registered workspace plugins",
      permission: "skills.packages.manage",
      order: 40,
    },
  ],
} as const satisfies WebserverPcModuleDefinition;
