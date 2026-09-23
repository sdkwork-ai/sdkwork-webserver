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
  /** `LAN` = same-subnet member; `TUNNEL` = API-only member reached through the reverse tunnel. */
  joinMode?: 'LAN' | 'TUNNEL';
  /** Service quality 0-100 derived from the latest heartbeat sample. */
  qualityScore?: number;
  /** Desired configuration revision; absent until the cluster publishes one. */
  desiredConfigRevision?: string;
  /** Configuration revision this instance last acknowledged as applied. */
  appliedConfigRevision?: string;
  /** Desired applications-manifest revision; absent until the cluster publishes one. */
  desiredApplicationsRevision?: string;
  /** Applications-manifest revision this instance last acknowledged as applied. */
  appliedApplicationsRevision?: string;
  /** Aggregate desired-vs-applied sync status for this instance. */
  syncStatus?: 'UNKNOWN' | 'IN_SYNC' | 'PENDING' | 'FAILED';
  /** Cordon switch: `false` keeps the instance serving but removes it from the routing pool. */
  routingEnabled?: boolean;
  /** Graceful drain in progress. */
  draining?: boolean;
  /** Taken out of the routing pool by the active prober after consecutive failures. */
  ejected?: boolean;
  /** Process restarts observed for this instance slot (auto-recovery evidence). */
  restartCount?: number;
  /** Operator labels. */
  labels?: Record<string, string>;
  /** Per-instance load balancing weight. */
  routingWeight?: number;
  /** Operator maintenance reason/context. */
  maintenanceNote?: string;
  /** Consecutive active-probe failures; reset on success. */
  probeFailures?: number;
  /** Active-probe target override. */
  probeUrl?: string;
  createdAt: string;
  updatedAt: string;
}
