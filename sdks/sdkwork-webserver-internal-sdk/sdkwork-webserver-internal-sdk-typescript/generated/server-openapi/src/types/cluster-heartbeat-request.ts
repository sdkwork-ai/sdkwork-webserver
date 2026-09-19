import type { Int64String } from './int64-string';

export interface ClusterHeartbeatRequest {
  status: number;
  healthState: 'HEALTHY' | 'DEGRADED' | 'UNHEALTHY' | 'UNKNOWN';
  uptimeSeconds: Int64String;
  buildVersion?: string;
  /** Resource metrics snapshot (CPU/memory/connections); bounded to 16 KiB. */
  metrics?: Record<string, unknown>;
}
