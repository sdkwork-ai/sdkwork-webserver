import type { Int64String } from './int64-string';

export interface ClusterInstanceResponse {
  id: string;
  clusterId: string;
  hostId: string;
  hostName?: string;
  name: string;
  role: 'GATEWAY' | 'MANAGEMENT' | 'DATA_PLANE' | 'WORKER' | 'OTHER';
  environment: 'development' | 'test' | 'staging' | 'production';
  processPid?: number;
  processStartedAt?: string;
  bindHost?: string;
  bindPort?: number;
  publicEndpoint?: string;
  buildVersion?: string;
  /** 0=offline, 1=online, 2=starting, 3=stopping, 4=error, 5=maintenance */
  status: number;
  healthState: 'HEALTHY' | 'DEGRADED' | 'UNHEALTHY' | 'UNKNOWN';
  lastHeartbeatAt?: string;
  lastOnlineAt?: string;
  uptimeSeconds: Int64String;
  /** Latest resource metrics snapshot (CPU/memory/connections). */
  metrics: Record<string, unknown>;
  createdAt: string;
  updatedAt: string;
}
