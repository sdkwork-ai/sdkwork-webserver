import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "cluster",
  label: "cluster",
  surface: "backend-admin",
  entries: [
    { resource: "cluster-overview", label: "Cluster", description: "Distributed cluster health and liveness overview", permission: "web.cluster.read", order: 1, path: "cluster" },
    { resource: "cluster-clusters", label: "Clusters", description: "Cluster grouping and heartbeat thresholds", permission: "web.cluster.read", order: 2, path: "cluster/clusters" },
    { resource: "cluster-hosts", label: "Cluster Hosts", description: "Host machines with system and network identity", permission: "web.cluster.read", order: 3, path: "cluster/hosts" },
    { resource: "cluster-instances", label: "Cluster Instances", description: "Webserver process instances and liveness", permission: "web.cluster.read", order: 4, path: "cluster/instances" },
    { resource: "cluster-events", label: "Cluster Events", description: "Cluster lifecycle event evidence", permission: "web.cluster.read", order: 5, path: "cluster/events" }
  ],
} as const satisfies WebserverPcModuleDefinition;
