export interface ServerResponse {
  id: string;
  name: string;
  host: string;
  tenantScopeHash: string;
  sshPort: number;
  /** 0=offline, 1=online */
  status: number;
  lastHeartbeatAt?: string;
  createdAt: string;
}
