import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "cloud-account",
  label: "cloud account",
  surface: "app-console",
  entries: [
    { resource: "cloud-accounts", label: "Cloud Accounts", description: "Provider accounts owned by you, your organization, or the tenant", permission: "iam.provider_accounts.read", order: 1 }
  ],
} as const satisfies WebserverPcModuleDefinition;
