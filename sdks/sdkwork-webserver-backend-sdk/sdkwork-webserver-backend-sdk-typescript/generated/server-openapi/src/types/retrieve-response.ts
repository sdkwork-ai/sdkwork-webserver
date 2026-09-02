import type { AgentSyncResponse } from './agent-sync-response';

export interface RetrieveResponse {
  code: 0;
  data: unknown & { item: AgentSyncResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
