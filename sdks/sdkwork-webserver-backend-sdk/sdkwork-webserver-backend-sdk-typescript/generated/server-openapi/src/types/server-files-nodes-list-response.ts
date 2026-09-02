import type { ServerFilesNode } from './server-files-node';

export interface ServerFilesNodesListResponse {
  code: 0;
  data: unknown & { items: ServerFilesNode[]; };
  /** Server-owned request correlation id. */
  traceId: string;
}
