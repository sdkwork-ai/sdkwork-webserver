import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "data-statistics",
  label: "data statistics",
  surface: "app-console",
  entries: [
    { resource: "dashboard", label: "Dashboard", description: "Traffic this edge served you over the recent window", permission: "web.traffic.read", order: 1 },
    { resource: "traffic-usage", label: "Traffic Statistics", description: "Requests, bytes, and per-app breakdown over a date window", permission: "web.traffic.read", order: 2 }
  ],
} as const satisfies WebserverPcModuleDefinition;
