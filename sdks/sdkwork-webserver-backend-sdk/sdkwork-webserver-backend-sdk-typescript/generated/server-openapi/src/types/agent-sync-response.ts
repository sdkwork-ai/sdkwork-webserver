import type { AgentCertificateBundle } from './agent-certificate-bundle';
import type { AgentNginxConfigBundle } from './agent-nginx-config-bundle';

export interface AgentSyncResponse {
  serverId: string;
  /** Stable SHA-256 fingerprint of active nginx configs and certificates for the tenant. */
  syncVersion: string;
  /** True when ifSyncVersion matched syncVersion; bundles are omitted to save bandwidth. */
  unchanged: boolean;
  nginxConfigs: AgentNginxConfigBundle[];
  certificates: AgentCertificateBundle[];
}
