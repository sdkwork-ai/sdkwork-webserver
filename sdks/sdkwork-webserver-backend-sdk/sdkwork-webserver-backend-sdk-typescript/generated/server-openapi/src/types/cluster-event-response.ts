export interface ClusterEventResponse {
  id: string;
  clusterId: string;
  hostId?: string;
  instanceId?: string;
  eventType: string;
  severity: 'INFO' | 'WARNING' | 'ERROR';
  message: string;
  detail: Record<string, unknown>;
  occurredAt: string;
  createdAt: string;
}
