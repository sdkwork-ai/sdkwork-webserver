export interface CreateServerResponse {
  id: string;
  name: string;
  host: string;
  tenantScopeHash: string;
  sshPort: number;
  /** 0=offline, 1=online */
  status: number;
  lastHeartbeatAt?: string;
  createdAt: string;
  /** Bootstrap agent credential; returned once at registration. */
  agentToken: string;
}
