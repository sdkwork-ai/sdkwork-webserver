import type { Int64String } from './int64-string';

export interface RootDomainResponse {
  id: string;
  hostname: string;
  status: number;
  subdomainCount: Int64String;
  boundSubdomainCount: Int64String;
  verifiedSubdomainCount: Int64String;
  httpsSubdomainCount: Int64String;
  activeDeploymentCount: Int64String;
  createdAt: string;
  updatedAt: string;
}
