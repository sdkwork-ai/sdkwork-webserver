import type { ServerOperationResult } from './server-operation-result';

export interface ServerFilesNodeOperationsCreateResponse201 {
  code: 0;
  data: unknown & ServerOperationResult;
  /** Server-owned request correlation id. */
  traceId: string;
}
