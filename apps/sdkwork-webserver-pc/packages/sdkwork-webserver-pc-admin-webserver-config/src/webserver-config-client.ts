import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  createWebserverAdminSdkClient,
  type WebserverAdminSdkClient,
  type WebserverConfigCatalog,
  type WebserverConfigFile,
  type WebserverConfigWriteResult,
} from "@sdkwork/webserver-pc-admin-core";

/**
 * Server Config API client.
 *
 * Delegates to the generated backend SDK (`webserverConfig` namespace,
 * consumed through the admin-core facade), which speaks the Web Server
 * configuration management contract:
 *
 *   GET  {base}/backend/v3/api/webserver_configs                -> catalog
 *   GET  {base}/backend/v3/api/webserver_configs/{configId}     -> file content + sha256
 *   PUT  {base}/backend/v3/api/webserver_configs/{configId}     -> validated atomic write
 *
 * The catalog id is the only address; there is no arbitrary path surface.
 * Writes are protected server-side by a timestamped backup and optional
 * optimistic concurrency (`expectedSha256`).
 */
export class WebserverConfigClient {
  private readonly client: WebserverAdminSdkClient;

  constructor(baseUrl: string, tokenManager: AuthTokenManager) {
    this.client = createWebserverAdminSdkClient(baseUrl, tokenManager);
  }

  async listConfigs(): Promise<WebserverConfigCatalog> {
    return this.client.webserverConfig.list();
  }

  async readConfig(configId: string): Promise<WebserverConfigFile> {
    return this.client.webserverConfig.retrieve(configId);
  }

  async updateConfig(
    configId: string,
    content: string,
    expectedSha256?: string,
  ): Promise<WebserverConfigWriteResult> {
    return this.client.webserverConfig.update(
      configId,
      { content, ...(expectedSha256 ? { expectedSha256 } : {}) },
      { idempotencyKey: newIdempotencyKey() },
    );
  }
}

function newIdempotencyKey(): string {
  const crypto = globalThis.crypto;
  if (typeof crypto?.randomUUID === "function") {
    try {
      return crypto.randomUUID();
    } catch {
      // Non-secure contexts may reject randomUUID; fall through.
    }
  }
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x40;
  bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80;
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export function createWebserverConfigClient(
  backendApiBaseUrl: string,
  tokenManager: AuthTokenManager,
): WebserverConfigClient {
  return new WebserverConfigClient(backendApiBaseUrl, tokenManager);
}
