export interface CreateClusterRequest {
  name: string;
  code: string;
  description?: string;
  heartbeatIntervalSeconds?: number;
  offlineThresholdSeconds?: number;
}
