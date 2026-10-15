import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "tunnel",
  label: "tunnel",
  surface: "backend-admin",
  // Tunnel machines are the connector nodes behind the reverse tunnel. The machine list survives disconnects, so an offline machine keeps its history and its switch; switching a machine off takes effect on its live session as well as on the next reconnect.
  // Declaring a route from here never carries a target: the machine activates the declaration only when it matches one of its own local templates and always dials its own configured target (PRD 45 SSRF boundary).
  entries: [
    { resource: "tunnel-machines", label: "Tunnel Machines", description: "Connector machines the tunnel gateway manages: liveness, routes, traffic and the operator switch", permission: "web.tunnel.read", order: 1, path: "tunnel/machines" }
  ],
} as const satisfies WebserverPcModuleDefinition;
