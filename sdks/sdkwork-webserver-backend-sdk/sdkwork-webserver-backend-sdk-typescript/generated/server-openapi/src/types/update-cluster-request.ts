export interface UpdateClusterRequest {
  name?: string;
  description?: string;
  status?: number;
  heartbeatIntervalSeconds?: number;
  offlineThresholdSeconds?: number;
  /** Request routing strategy across the cluster's instances. */
  lbStrategy?: 'round_robin' | 'weighted_round_robin' | 'least_connections' | 'random' | 'random_two_choices' | 'ip_hash' | 'consistent_hash';
  /** Service domains auto-routed to this cluster's instances; replaces the whole list when present. */
  servedDomains?: string[];
}
