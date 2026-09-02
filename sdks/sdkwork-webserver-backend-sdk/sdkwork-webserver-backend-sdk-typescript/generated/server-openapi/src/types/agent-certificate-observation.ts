export interface AgentCertificateObservation {
  certificateId: string;
  fingerprint: string;
  syncVersion: string;
  state: 'STAGED' | 'ACTIVE' | 'SERVED' | 'FAILED';
  observedAt: string;
  failureCode?: string;
}
