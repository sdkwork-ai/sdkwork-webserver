import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "sandbox",
  label: "sandbox",
  surface: "app-console",
  entries: [
    { resource: "sandbox-instances", label: "VM Instances", description: "Virtual machine (VM) instances provisioned by the authenticated user", permission: "web.sandbox.read", order: 1 }
  ],
} as const satisfies WebserverPcModuleDefinition;
