import type { Int64String } from './int64-string';

export interface ClusterResponse {
  id: string;
  name: string;
  code: string;
  description?: string;
  /** 0=inactive, 1=active */
  status: number;
  heartbeatIntervalSeconds: number;
  offlineThresholdSeconds: number;
  hostCount: Int64String;
  instanceCount: Int64String;
  onlineInstanceCount: Int64String;
  createdAt: string;
  updatedAt: string;
}
