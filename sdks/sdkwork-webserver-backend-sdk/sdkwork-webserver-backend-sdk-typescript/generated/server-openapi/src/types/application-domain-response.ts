import type { DomainDeploymentResponse } from './domain-deployment-response';

export interface ApplicationDomainResponse {
  id: string;
  hostname: string;
  rootDomainId?: string;
  recordName?: string;
  applicationId?: string;
  applicationName?: string;
  certificateCount: string;
  isPrimary: boolean;
  isVerified: boolean;
  sslEnabled: boolean;
  sslProvider?: string;
  status: number;
  latestDeployment?: DomainDeploymentResponse;
  createdAt: string;
  updatedAt?: string;
}
