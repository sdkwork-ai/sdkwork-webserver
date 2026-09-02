export interface AgentCertificateBundle {
  certificateId: string;
  certName: string;
  fingerprint: string;
  hostnames: string[];
  fullchainPem: string;
  privkeyPem: string;
}
