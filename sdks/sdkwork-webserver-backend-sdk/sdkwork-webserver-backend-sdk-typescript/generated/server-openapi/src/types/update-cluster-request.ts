export interface UpdateClusterRequest {
  name?: string;
  description?: string;
  status?: number;
  heartbeatIntervalSeconds?: number;
  offlineThresholdSeconds?: number;
}
