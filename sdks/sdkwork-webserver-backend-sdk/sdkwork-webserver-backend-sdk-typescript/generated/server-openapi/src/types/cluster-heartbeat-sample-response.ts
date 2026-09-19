export interface ClusterHeartbeatSampleResponse {
  id: string;
  status: number;
  latencyMs?: number;
  /** Resource metrics snapshot captured at heartbeat time. */
  metrics: Record<string, unknown>;
  reportedAt: string;
}
