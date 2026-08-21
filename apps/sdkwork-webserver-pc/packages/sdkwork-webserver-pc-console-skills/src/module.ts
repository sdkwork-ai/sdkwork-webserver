import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "skills",
  label: "skills",
  surface: "app-console",
  entries: [
    {
      resource: "skills",
      label: "My Skills",
      description: "Create, upload, and manage skill packages you own",
      permission: "skills.marketplace.read",
      order: 1,
    },
  ],
} as const satisfies WebserverPcModuleDefinition;
