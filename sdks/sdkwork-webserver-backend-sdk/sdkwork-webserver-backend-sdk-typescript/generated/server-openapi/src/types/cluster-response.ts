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
  /** Request routing strategy across the cluster's instances. */
  lbStrategy?: 'round_robin' | 'weighted_round_robin' | 'least_connections' | 'random' | 'random_two_choices' | 'ip_hash' | 'consistent_hash';
  /** Service domains auto-routed to this cluster's instances. */
  servedDomains?: string[];
  createdAt: string;
  updatedAt: string;
}
