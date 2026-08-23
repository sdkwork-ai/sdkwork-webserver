import { buildAuthHeaders, type AuthTokenManager } from "@sdkwork/sdk-common";
import type {
  ServerDirectoryListing,
  ServerEntry,
  ServerFileContent,
  ServerNode,
  ServerOperationResult,
  ServerProjectOperations,
} from "./server-files-types.ts";
import { classifyListing, detectProjectType } from "./project-detection.ts";

/**
 * ServerFilesExplorer API client.
 *
 * Speaks the Web Server backend local-project / node file-system contract:
 *
 *   GET  {base}/backend/v3/api/server-files/nodes              -> node list
 *   GET  {base}/backend/v3/api/server-files/nodes/{nodeId}/browse?path=...
 *                                                              -> directory listing
 *   GET  {base}/backend/v3/api/server-files/nodes/{nodeId}/read?path=...
 *                                                              -> file content
 *   GET  {base}/backend/v3/api/server-files/nodes/{nodeId}/operations?path=...
 *                                                              -> per-project operations
 *   POST {base}/backend/v3/api/server-files/nodes/{nodeId}/operations
 *                                                              -> run an operation
 *
 * All mutations require a matching IAM permission granted by the backend
 * route metadata (`web.servers.files.write`, `web.servers.files.deploy`,
 * etc.). The client attaches the IAM dual-token session to every request.
 */
export class ServerFilesClient {
  constructor(
    private readonly baseUrl: string,
    private readonly tokenManager: AuthTokenManager,
  ) {}

  async listNodes(): Promise<ServerNode[]> {
    const data = await this.request<{ items: ServerNode[] }>("/backend/v3/api/server-files/nodes");
    return data.items ?? [];
  }

  async browseDirectory(nodeId: string, path: string): Promise<ServerDirectoryListing> {
    const query = encodeQuery({ path });
    const data = await this.request<{
      nodeId: string;
      path: string;
      parentPath: string | null;
      entries: ServerEntry[];
    }>(`/backend/v3/api/server-files/nodes/${encodeURIComponent(nodeId)}/browse?${query}`);
    const entries = classifyListing({ path: data.path, entries: data.entries });
    return { ...data, entries };
  }

  async readFile(nodeId: string, path: string): Promise<ServerFileContent> {
    const query = encodeQuery({ path });
    return this.request<ServerFileContent>(
      `/backend/v3/api/server-files/nodes/${encodeURIComponent(nodeId)}/read?${query}`,
    );
  }

  async operationsFor(nodeId: string, path: string): Promise<ServerProjectOperations> {
    const query = encodeQuery({ path });
    return this.request<ServerProjectOperations>(
      `/backend/v3/api/server-files/nodes/${encodeURIComponent(nodeId)}/operations?${query}`,
    );
  }

  async runOperation(
    nodeId: string,
    path: string,
    operationId: string,
  ): Promise<ServerOperationResult> {
    return this.request<ServerOperationResult>(
      `/backend/v3/api/server-files/nodes/${encodeURIComponent(nodeId)}/operations`,
      {
        method: "POST",
        body: JSON.stringify({ path, operationId }),
      },
    );
  }

  /** Best-effort local project-type detection for a directory entry. */
  static detectProjectType(entries: readonly ServerEntry[]) {
    return detectProjectType(entries);
  }

  private async request<T>(path: string, init: RequestInit = {}): Promise<T> {
    const headers: Record<string, string> = {
      ...buildAuthHeaders("dual-token", undefined, this.tokenManager),
      "Content-Type": "application/json",
      ...(init.headers as Record<string, string> | undefined),
    };
    const response = await fetch(`${this.baseUrl}${path}`, { ...init, headers });
    if (!response.ok) {
      throw new Error(`Server files request failed (${response.status})`);
    }
    if (response.status === 204) {
      return undefined as T;
    }
    return (await response.json()) as T;
  }
}

function encodeQuery(params: Record<string, string>): string {
  return Object.entries(params)
    .filter(([, value]) => value !== undefined && value !== null)
    .map(([key, value]) => `${encodeURIComponent(key)}=${encodeURIComponent(value)}`)
    .join("&");
}

export function createServerFilesClient(
  backendApiBaseUrl: string,
  tokenManager: AuthTokenManager,
): ServerFilesClient {
  return new ServerFilesClient(backendApiBaseUrl, tokenManager);
}
