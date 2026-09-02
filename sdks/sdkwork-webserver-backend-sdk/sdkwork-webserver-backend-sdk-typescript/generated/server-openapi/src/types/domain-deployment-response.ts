export interface DomainDeploymentResponse {
  id: string;
  status: number;
  environment: string;
  versionTag?: string;
  completedAt?: string;
  createdAt: string;
}
