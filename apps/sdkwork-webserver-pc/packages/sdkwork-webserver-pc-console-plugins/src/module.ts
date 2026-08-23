import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "plugins",
  label: "plugins",
  surface: "app-console",
  entries: [
    {
      resource: "plugins",
      label: "My Plugins",
      description: "Register plugins from Git or archive uploads",
      permission: "web.applications.read",
      order: 40,
    },
  ],
} as const satisfies WebserverPcModuleDefinition;
