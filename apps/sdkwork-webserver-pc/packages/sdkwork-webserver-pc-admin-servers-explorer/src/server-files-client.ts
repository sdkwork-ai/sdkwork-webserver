import type { AuthTokenManager } from "@sdkwork/sdk-common";
import {
  createWebserverAdminSdkClient,
  type ServerDirectoryListing as GeneratedDirectoryListing,
  type ServerEntry as GeneratedEntry,
  type ServerFileContent as GeneratedFileContent,
  type ServerFilesNode as GeneratedNode,
  type ServerOperationResult as GeneratedOperationResult,
  type ServerProjectOperations as GeneratedProjectOperations,
  type WebserverAdminSdkClient,
} from "@sdkwork/webserver-pc-admin-core";
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
 * Delegates to the generated backend SDK (`@sdkwork/webserver-backend-sdk`,
 * `serverFile` namespace), which speaks the Web Server backend
 * local-project / node file-system contract:
 *
 *   GET  {base}/backend/v3/api/server_files/nodes              -> node list
 *   GET  {base}/backend/v3/api/server_files/nodes/{nodeId}/directory?path=...
 *                                                              -> directory listing
 *   GET  {base}/backend/v3/api/server_files/nodes/{nodeId}/files/{filePath}
 *                                                              -> file content
 *   GET  {base}/backend/v3/api/server_files/nodes/{nodeId}/operations?path=...
 *                                                              -> per-project operations
 *   POST {base}/backend/v3/api/server_files/nodes/{nodeId}/operations
 *                                                              -> run an operation
 *
 * All mutations require a matching IAM permission granted by the backend
 * route metadata (`web.servers.files.write`, `web.servers.files.deploy`,
 * etc.). The generated HttpClient attaches the IAM dual-token session to
 * every request. Wire responses are normalized to the local domain model
 * (`server-files-types.ts`) so the UI never depends on raw wire shapes —
 * notably int64 sizes arrive as strings and are decoded to numbers here.
 */
export class ServerFilesClient {
  private readonly client: WebserverAdminSdkClient;

  constructor(baseUrl: string, tokenManager: AuthTokenManager) {
    this.client = createWebserverAdminSdkClient(baseUrl, tokenManager);
  }

  async listNodes(): Promise<ServerNode[]> {
    const data = await this.client.serverFile.nodes.list();
    return (data.items ?? []).map(mapNode);
  }

  async browseDirectory(nodeId: string, path: string): Promise<ServerDirectoryListing> {
    const data = await this.client.serverFile.nodes.directory.list(nodeId, { path });
    const entries = classifyListing({ path: data.path, entries: data.entries.map(mapEntry) });
    return { nodeId: data.nodeId, path: data.path, parentPath: data.parentPath, entries };
  }

  async readFile(nodeId: string, path: string): Promise<ServerFileContent> {
    const data = await this.client.serverFile.nodes.file.retrieve(nodeId, path);
    return mapFileContent(data);
  }

  async operationsFor(nodeId: string, path: string): Promise<ServerProjectOperations> {
    const data = await this.client.serverFile.nodes.operations.list(nodeId, { path });
    return mapProjectOperations(data);
  }

  async runOperation(
    nodeId: string,
    path: string,
    operationId: string,
  ): Promise<ServerOperationResult> {
    const data = await this.client.serverFile.nodes.operations.create(nodeId, {
      path,
      operationId,
    });
    return mapOperationResult(data);
  }

  /** Best-effort local project-type detection for a directory entry. */
  static detectProjectType(entries: readonly ServerEntry[]) {
    return detectProjectType(entries);
  }
}

function mapNode(node: GeneratedNode): ServerNode {
  return { ...node };
}

function mapEntry(entry: GeneratedEntry): ServerEntry {
  return {
    ...entry,
    size: entry.size === undefined ? undefined : Number(entry.size),
  };
}

function mapFileContent(content: GeneratedFileContent): ServerFileContent {
  return {
    nodeId: content.nodeId,
    path: content.path,
    content: content.content,
    size: Number(content.size),
  };
}

function mapProjectOperations(operations: GeneratedProjectOperations): ServerProjectOperations {
  return { ...operations };
}

function mapOperationResult(result: GeneratedOperationResult): ServerOperationResult {
  return {
    operationId: result.operationId,
    exitCode: result.exitCode === undefined || result.exitCode === null ? undefined : result.exitCode,
    stdout: result.stdout,
    stderr: result.stderr,
  };
}

export function createServerFilesClient(
  backendApiBaseUrl: string,
  tokenManager: AuthTokenManager,
): ServerFilesClient {
  return new ServerFilesClient(backendApiBaseUrl, tokenManager);
}
