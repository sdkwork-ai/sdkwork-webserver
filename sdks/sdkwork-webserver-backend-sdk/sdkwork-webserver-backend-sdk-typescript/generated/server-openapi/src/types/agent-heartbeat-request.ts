import type { AgentCertificateObservation } from './agent-certificate-observation';

export interface AgentHeartbeatRequest {
  agentVersion?: string;
  nginxEnabled?: boolean;
  /** Number of active nginx configs reported by the agent as a string. */
  activeConfigs?: string;
  /** Last successfully applied syncVersion reported by the edge agent. */
  lastSyncVersion?: string;
  certificateObservations?: AgentCertificateObservation[];
}
