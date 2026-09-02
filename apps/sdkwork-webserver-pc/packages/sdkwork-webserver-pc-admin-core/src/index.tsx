import { normalizeWebserverPage, type WebserverResourceAction, type WebserverResourceActionContext, type WebserverResourceDataSource, type WebserverResourceRegistry } from "@sdkwork/webserver-pc-commons";
import {
  createClient,
  type CreateNginxConfigRequest,
  type CreateServerRequest,
  type SdkworkBackendClient,
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
  ServerDirectoryListing,
  ServerEntry,
  ServerFileContent,
  ServerFilesNode,
  ServerOperationResult,
  ServerProjectOperations,
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
