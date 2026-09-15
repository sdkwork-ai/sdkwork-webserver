import type { WebserverPcModuleDefinition } from "@sdkwork/webserver-pc-commons";

export const webserverModule = {
  id: "storage",
  label: "storage",
  surface: "backend-admin",
  entries: [
    { resource: "storage-providers", label: "Storage Providers", description: "Configure and manage the object storage backends Drive writes to", permission: "drive.storage.admin", order: 1, path: "storage/providers" },
    { resource: "storage-kinds", label: "Provider Catalog", description: "Enable or disable the storage provider kinds operators may choose", permission: "drive.storage.admin", order: 2, path: "storage/kinds" },
    { resource: "storage-buckets", label: "Buckets", description: "Inspect and create the bucket behind each storage provider", permission: "drive.storage.admin", order: 3, path: "storage/buckets" },
    { resource: "storage-bindings", label: "Bindings", description: "Route each space type to a default storage provider", permission: "drive.storage.admin", order: 4, path: "storage/bindings" }
  ],
} as const satisfies WebserverPcModuleDefinition;
