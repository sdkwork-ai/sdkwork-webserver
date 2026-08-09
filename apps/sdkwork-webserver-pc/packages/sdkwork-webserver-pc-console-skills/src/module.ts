import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "skills",
  label: "skills",
  surface: "app-console",
  entries: [
    { resource: "skills", label: "My Skills", description: "Skill packages owned by the authenticated user", permission: "skills.marketplace.read", order: 1 }
  ],
} as const satisfies WebserverPcModuleDefinition;
