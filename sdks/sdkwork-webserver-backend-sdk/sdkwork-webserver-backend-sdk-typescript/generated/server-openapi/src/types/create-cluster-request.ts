export interface CreateClusterRequest {
  name: string;
  code: string;
  description?: string;
  heartbeatIntervalSeconds?: number;
  offlineThresholdSeconds?: number;
  /** Request routing strategy; defaults to `round_robin` when omitted. */
  lbStrategy?: 'round_robin' | 'weighted_round_robin' | 'least_connections' | 'random' | 'random_two_choices' | 'ip_hash' | 'consistent_hash';
  /** Service domains auto-routed to this cluster's instances. */
  servedDomains?: string[];
}
