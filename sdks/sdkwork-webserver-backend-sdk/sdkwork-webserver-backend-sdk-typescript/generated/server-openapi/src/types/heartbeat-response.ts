import type { AgentHeartbeatResponse } from './agent-heartbeat-response';

export interface HeartbeatResponse {
  code: 0;
  data: unknown & { item: AgentHeartbeatResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
