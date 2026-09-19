export interface ClusterPeer {
  instanceId: string;
  name: string;
  role: string;
  status: number;
  hostName: string;
  remoteIp?: string;
  publicEndpoint?: string;
  environment: string;
  buildVersion?: string;
  lastHeartbeatAt?: string;
}
