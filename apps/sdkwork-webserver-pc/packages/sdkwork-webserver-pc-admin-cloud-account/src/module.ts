import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "cloud-account",
  label: "cloud account",
  surface: "backend-admin",
  entries: [
    { resource: "cloud-accounts", label: "Cloud Accounts", description: "Provider accounts across the personal, organization, tenant, and platform levels", permission: "iam.provider_accounts.read", order: 1 }
  ],
} as const satisfies WebserverPcModuleDefinition;
