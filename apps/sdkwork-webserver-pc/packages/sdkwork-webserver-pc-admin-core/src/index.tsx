import { normalizeWebserverPage, type WebserverResourceAction, type WebserverResourceActionContext, type WebserverResourceDataSource, type WebserverResourceRegistry } from "@sdkwork/webserver-pc-commons";
import {
  createClient,
  type CreateClusterRequest,
  type CreateNginxConfigRequest,
  type CreateServerRequest,
  type SdkworkBackendClient,
  type UpdateClusterInstanceRequest,
  type UpdateClusterHostRequest,
  type UpdateClusterRequest,
  type UpdateNginxConfigRequest,
} from "@sdkwork/webserver-backend-sdk";
import type { AuthTokenManager } from "@sdkwork/sdk-common";
import { createContext, useContext, type ReactNode } from "react";
import { createDriveAppClient, type SdkworkDriveAppClient } from "@sdkwork/drive-app-sdk";

export type { SdkworkDriveAppClient };
export { createDriveAppClient };

// Public wire-type surface for feature packages: they consume the generated
// backend SDK only through these core exports, never via direct imports
// (frontend composition feature-package import rule).
export type {
  ClusterEventResponse,
  ClusterHostResponse,
  ClusterInstanceResponse,
  ClusterOverviewResponse,
  ClusterResponse,
  ServerDirectoryListing,
  ServerEntry,
  ServerFileContent,
  ServerFilesNode,
  ServerOperationResult,
  ServerProjectOperations,
  WebserverConfigCatalog,
  WebserverConfigEntry,
  WebserverConfigFile,
  WebserverConfigWriteRequest,
  WebserverConfigWriteResult,
} from "@sdkwork/webserver-backend-sdk";

export type WebserverAdminSdkClient = SdkworkBackendClient;
const Context = createContext<WebserverAdminSdkClient | null>(null);
export function createWebserverAdminSdkClient(baseUrl: string, tokenManager: AuthTokenManager): WebserverAdminSdkClient { return createClient({ baseUrl, authMode: "dual-token", platform: "pc", tokenManager }); }
export function WebserverAdminSdkProvider({ children, client }: { children: ReactNode; client: WebserverAdminSdkClient }) { return <Context.Provider value={client}>{children}</Context.Provider>; }
export function useWebserverAdminSdk(): WebserverAdminSdkClient { const client = useContext(Context); if (!client) throw new Error("WebserverAdminSdkProvider is required"); return client; }

export function createWebserverAdminRegistry(client: WebserverAdminSdkClient): WebserverResourceRegistry {
  return {
    nginx: source((query) => client.nginx.configs.list({ page: query.page, pageSize: query.pageSize }), [
      action("create", "Create config", { siteId: "", configType: 1, configName: "", configContent: "" }, async (context) => client.nginx.configs.create(createNginxConfigRequest(context.body), idempotencyParams(context)), { fieldOptions: { configType: [1, 2, 3, 4] }, permission: "web.nginx.write", requiredFields: ["siteId", "configName", "configContent"] }),
      action("update", "Update", { configName: "", configContent: "" }, async (context) => client.nginx.configs.update(selectedId(context, "id"), updateNginxConfigRequest(context.body), idempotencyParams(context)), { permission: "web.nginx.write", selection: true }),
      action("validate", "Validate", {}, (context) => client.nginx.configs.validate(selectedId(context, "id")), { permission: "web.nginx.write", selection: true }),
      action("deploy", "Deploy", {}, (context) => client.nginx.configs.deploy(selectedId(context, "id"), idempotencyParams(context)), { dangerous: true, permission: "web.nginx.write", selection: true }),
      action("reload", "Reload runtime", {}, (context) => client.nginx.reload.create(idempotencyParams(context)), { dangerous: true, permission: "web.nginx.write" }),
    ]),
    servers: source((query) => client.server.list({ cursor: query.cursor, pageSize: query.pageSize }), [
      action("create", "Register server", { name: "", host: "", sshPort: 22, tenantScopeHash: "" }, async (context) => client.server.create(createServerRequest(context.body), idempotencyParams(context)), { permission: "web.servers.write", requiredFields: ["name", "host", "tenantScopeHash"], resultFields: ["agentToken", "id", "name", "host", "sshPort"] }),
    ]),
    "cluster-clusters": source((query) => client.cluster.list({ page: query.page, pageSize: query.pageSize }), [
      action("create", "Create cluster", { name: "", code: "", description: "", heartbeatIntervalSeconds: 15, offlineThresholdSeconds: 60 }, async (context) => client.cluster.create(createClusterRequest(context.body), idempotencyParams(context)), { permission: "web.cluster.write", requiredFields: ["name", "code"] }),
      action("update", "Update", { description: "", heartbeatIntervalSeconds: 15, offlineThresholdSeconds: 60 }, async (context) => client.cluster.update(selectedId(context, "id"), updateClusterRequest(context.body), idempotencyParams(context)), { permission: "web.cluster.write", selection: true }),
      action("delete", "Delete cluster", {}, (context) => client.cluster.delete(selectedId(context, "id"), idempotencyParams(context)), { dangerous: true, permission: "web.cluster.write", selection: true }),
    ]),
    "cluster-hosts": {
      ...source((query) => client.cluster.hosts.list({ cursor: query.cursor, pageSize: query.pageSize, clusterId: filterValue(query.filters, "clusterId"), status: optionalIntegerFilter(query.filters, "status") }), [
        action("delete", "Remove host", {}, (context) => client.cluster.hosts.delete(selectedId(context, "id"), idempotencyParams(context)), { dangerous: true, permission: "web.cluster.write", selection: true }),
      ]),
      filters: [
        { id: "clusterId", type: "text" },
        { id: "status", type: "select", fieldOptions: ["0", "1", "2", "3", "4"] },
      ],
    },
    "cluster-instances": {
      ...source((query) => client.cluster.instances.list({ cursor: query.cursor, pageSize: query.pageSize, clusterId: filterValue(query.filters, "clusterId"), hostId: filterValue(query.filters, "hostId"), status: optionalIntegerFilter(query.filters, "status"), healthState: healthStateFilter(query.filters) }), [
        action("maintain", "Mark maintenance", {}, async (context) => client.cluster.instances.update(selectedId(context, "id"), maintenanceRequest(), idempotencyParams(context)), { permission: "web.cluster.write", selection: true }),
        action("delete", "Unregister", {}, (context) => client.cluster.instances.delete(selectedId(context, "id"), idempotencyParams(context)), { dangerous: true, permission: "web.cluster.write", selection: true }),
      ]),
      filters: [
        { id: "clusterId", type: "text" },
        { id: "hostId", type: "text" },
        { id: "status", type: "select", fieldOptions: ["0", "1", "2", "3", "4", "5"] },
        { id: "healthState", type: "select", fieldOptions: ["HEALTHY", "DEGRADED", "UNHEALTHY", "UNKNOWN"] },
      ],
    },
    "cluster-events": {
      ...source((query) => client.cluster.events.list({ cursor: query.cursor, pageSize: query.pageSize, clusterId: filterValue(query.filters, "clusterId"), severity: severityFilter(query.filters) }), []),
      filters: [
        { id: "clusterId", type: "text" },
        { id: "severity", type: "select", fieldOptions: ["INFO", "WARNING", "ERROR"] },
      ],
    },
    diagnostics: source(async () => client.nginx.status.retrieve(), [action("reload", "Reload runtime", {}, (context) => client.nginx.reload.create(idempotencyParams(context)), { dangerous: true, permission: "web.nginx.write" })]),
    audit: {
      ...source((query) => client.audit.auditLogs.list({
        cursor: query.cursor,
        pageSize: query.pageSize,
        targetType: filterValue(query.filters, "targetType"),
        action: filterValue(query.filters, "action") ?? query.search,
        operatorId: filterValue(query.filters, "operatorId"),
        startDate: filterValue(query.filters, "startDate"),
        endDate: filterValue(query.filters, "endDate"),
      }), []),
      filters: [
        { id: "targetType", type: "select", fieldOptions: ["site", "domain", "deployment", "certificate", "nginx_config", "server"] },
        { id: "action", type: "text" },
        { id: "operatorId", type: "text" },
        { id: "startDate", type: "date" },
        { id: "endDate", type: "date" },
      ],
    },
  };
}

function source(load: WebserverResourceDataSource["load"] extends (query: infer Q) => Promise<unknown> ? (query: Q) => Promise<unknown> : never, actions: readonly WebserverResourceAction[]): WebserverResourceDataSource { return { actions, async load(query) { return normalizeWebserverPage(await load(query)); } }; }
function action(id: string, label: string, bodyTemplate: Record<string, unknown>, execute: WebserverResourceAction["execute"], options: Omit<WebserverResourceAction, "bodyTemplate" | "execute" | "id" | "label" | "requiresSelection"> & { selection?: boolean } = {}): WebserverResourceAction { return { id, label, bodyTemplate, execute, ...options, requiresSelection: options.selection }; }
function selectedId(context: WebserverResourceActionContext, key: string): string { const value = context.selectedItem?.[key]; if (typeof value !== "string" && typeof value !== "number") throw new Error(`${key} is unavailable`); return String(value); }
function idempotencyParams(context: WebserverResourceActionContext): { idempotencyKey: string } { const idempotencyKey = context.idempotencyKey?.trim(); if (!idempotencyKey) throw new Error("Idempotency key is required"); return { idempotencyKey }; }
function filterValue(filters: Readonly<Record<string, string>> | undefined, key: string): string | undefined { const value = filters?.[key]?.trim(); return value || undefined; }

function createNginxConfigRequest(body: Readonly<Record<string, unknown>>): CreateNginxConfigRequest {
  return {
    siteId: requiredText(body.siteId, "Site ID", 64),
    configType: nginxConfigType(body.configType),
    configName: requiredText(body.configName, "Configuration name", 200),
    configContent: nginxConfigContent(body.configContent),
  };
}

function updateNginxConfigRequest(body: Readonly<Record<string, unknown>>): UpdateNginxConfigRequest {
  const configName = optionalText(body.configName, "Configuration name", 200);
  const configContent = optionalNginxConfigContent(body.configContent);
  if (configName === undefined && configContent === undefined) {
    throw new Error("At least one configuration field is required");
  }
  return { configName, configContent };
}

function createClusterRequest(body: Readonly<Record<string, unknown>>): CreateClusterRequest {
  const request: CreateClusterRequest = {
    name: requiredText(body.name, "Cluster name", 100),
    code: requiredClusterCode(body.code),
  };
  const description = optionalText(body.description, "Description", 500);
  if (description !== undefined) request.description = description;
  const heartbeatIntervalSeconds = optionalInteger(body.heartbeatIntervalSeconds);
  if (heartbeatIntervalSeconds !== undefined) {
    if (heartbeatIntervalSeconds < 5 || heartbeatIntervalSeconds > 600) throw new Error("Heartbeat interval must be between 5 and 600 seconds");
    request.heartbeatIntervalSeconds = heartbeatIntervalSeconds;
  }
  const offlineThresholdSeconds = optionalInteger(body.offlineThresholdSeconds);
  if (offlineThresholdSeconds !== undefined) {
    if (offlineThresholdSeconds < 10 || offlineThresholdSeconds > 3600) throw new Error("Offline threshold must be between 10 and 3600 seconds");
    request.offlineThresholdSeconds = offlineThresholdSeconds;
  }
  return request;
}

function updateClusterRequest(body: Readonly<Record<string, unknown>>): UpdateClusterRequest {
  const request: UpdateClusterRequest = {};
  const description = optionalText(body.description, "Description", 500);
  if (description !== undefined) request.description = description;
  const heartbeatIntervalSeconds = optionalInteger(body.heartbeatIntervalSeconds);
  if (heartbeatIntervalSeconds !== undefined) {
    if (heartbeatIntervalSeconds < 5 || heartbeatIntervalSeconds > 600) throw new Error("Heartbeat interval must be between 5 and 600 seconds");
    request.heartbeatIntervalSeconds = heartbeatIntervalSeconds;
  }
  const offlineThresholdSeconds = optionalInteger(body.offlineThresholdSeconds);
  if (offlineThresholdSeconds !== undefined) {
    if (offlineThresholdSeconds < 10 || offlineThresholdSeconds > 3600) throw new Error("Offline threshold must be between 10 and 3600 seconds");
    request.offlineThresholdSeconds = offlineThresholdSeconds;
  }
  if (Object.keys(request).length === 0) throw new Error("At least one cluster field is required");
  return request;
}

function updateClusterHostRequest(body: Readonly<Record<string, unknown>>): UpdateClusterHostRequest {
  const request: UpdateClusterHostRequest = {};
  const name = optionalText(body.name, "Host name", 100);
  if (name !== undefined) request.name = name;
  const clusterId = optionalText(body.clusterId, "Target cluster", 64);
  if (clusterId !== undefined) request.clusterId = clusterId;
  if (Object.keys(request).length === 0) throw new Error("At least one host field is required");
  return request;
}

function updateClusterInstanceRequest(body: Readonly<Record<string, unknown>>): UpdateClusterInstanceRequest {
  const request: UpdateClusterInstanceRequest = {};
  const name = optionalText(body.name, "Instance name", 100);
  if (name !== undefined) request.name = name;
  const publicEndpoint = optionalText(body.publicEndpoint, "Public endpoint", 255);
  if (publicEndpoint !== undefined) request.publicEndpoint = publicEndpoint;
  const status = optionalInteger(body.status);
  if (status !== undefined) {
    if (status < 0 || status > 5) throw new Error("Instance status must be between 0 and 5");
    request.status = status;
  }
  if (Object.keys(request).length === 0) throw new Error("At least one instance field is required");
  return request;
}

function maintenanceRequest(): UpdateClusterInstanceRequest {
  return { status: 5 };
}

function requiredClusterCode(value: unknown): string {
  const code = requiredText(value, "Cluster code", 64);
  if (!/^[a-z0-9][a-z0-9._-]{0,63}$/.test(code)) throw new Error("Cluster code must be lowercase letters, digits, dot, underscore, or dash");
  return code;
}

function optionalInteger(value: unknown): number | undefined {
  if (value === undefined || value === null || value === "") return undefined;
  const parsed = Number(value);
  if (!Number.isInteger(parsed)) throw new Error("Value must be an integer");
  return parsed;
}

function optionalIntegerFilter(filters: Readonly<Record<string, string>> | undefined, key: string): number | undefined {
  const value = filterValue(filters, key);
  if (value === undefined) return undefined;
  const parsed = Number(value);
  if (!Number.isInteger(parsed)) return undefined;
  return parsed;
}

function healthStateFilter(filters: Readonly<Record<string, string>> | undefined): "HEALTHY" | "DEGRADED" | "UNHEALTHY" | "UNKNOWN" | undefined {
  const value = filterValue(filters, "healthState");
  return value === "HEALTHY" || value === "DEGRADED" || value === "UNHEALTHY" || value === "UNKNOWN" ? value : undefined;
}

function severityFilter(filters: Readonly<Record<string, string>> | undefined): "INFO" | "WARNING" | "ERROR" | undefined {
  const value = filterValue(filters, "severity");
  return value === "INFO" || value === "WARNING" || value === "ERROR" ? value : undefined;
}

function createServerRequest(body: Readonly<Record<string, unknown>>): CreateServerRequest {
  const host = requiredText(body.host, "Host", 255);
  if (/\s/.test(host)) throw new Error("Host must not contain whitespace");
  const tenantScopeHash = requiredText(body.tenantScopeHash, "Tenant scope hash", 64);
  if (!/^[a-f0-9]{64}$/.test(tenantScopeHash)) {
    throw new Error("Tenant scope hash must be a lowercase SHA-256 digest");
  }
  return {
    name: requiredText(body.name, "Server name", 100),
    host,
    tenantScopeHash,
    sshPort: boundedInteger(body.sshPort, "SSH port", 1, 65_535),
  };
}

function nginxConfigType(value: unknown): 1 | 2 | 3 | 4 {
  const parsed = Number(value);
  if (parsed === 1 || parsed === 2 || parsed === 3 || parsed === 4) return parsed;
  throw new Error("Configuration type is invalid");
}

function nginxConfigContent(value: unknown): string {
  if (typeof value !== "string" || value.length === 0 || value.includes("\0")) {
    throw new Error("Configuration content is required");
  }
  if (value.length > MAX_NGINX_CONFIG_BYTES || UTF8_ENCODER.encode(value).byteLength > MAX_NGINX_CONFIG_BYTES) {
    throw new Error("Configuration content must not exceed 1 MiB");
  }
  return value;
}

function optionalNginxConfigContent(value: unknown): string | undefined {
  if (value === undefined || value === null || value === "") return undefined;
  return nginxConfigContent(value);
}

function requiredText(value: unknown, label: string, maximum: number): string {
  const text = optionalText(value, label, maximum);
  if (!text) throw new Error(`${label} is required`);
  return text;
}

function optionalText(value: unknown, label: string, maximum: number): string | undefined {
  if (value === undefined || value === null || value === "") return undefined;
  if (typeof value !== "string") throw new Error(`${label} is invalid`);
  const text = value.trim();
  if (!text || text.length > maximum || /[\u0000-\u001f\u007f]/.test(text)) {
    throw new Error(`${label} is invalid`);
  }
  return text;
}

function boundedInteger(value: unknown, label: string, minimum: number, maximum: number): number {
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed < minimum || parsed > maximum) {
    throw new Error(`${label} must be between ${minimum} and ${maximum}`);
  }
  return parsed;
}

const MAX_NGINX_CONFIG_BYTES = 1024 * 1024;
const UTF8_ENCODER = new TextEncoder();
