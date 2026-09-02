import type { ServerProjectOperations } from './server-project-operations';

export interface ServerFilesNodeOperationsListResponse {
  code: 0;
  data: unknown & ServerProjectOperations;
  /** Server-owned request correlation id. */
  traceId: string;
}
