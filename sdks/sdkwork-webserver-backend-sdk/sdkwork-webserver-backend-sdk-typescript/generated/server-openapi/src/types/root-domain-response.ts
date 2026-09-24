import type { Int64String } from './int64-string';

export interface RootDomainResponse {
  id: string;
  hostname: string;
  /** Operator-facing label; the apex hostname remains the identity. */
  displayName?: string;
  /** DNS provider declaration, or the literal manual when records are published by hand. */
  dnsProvider?: string;
  /** Provider-side zone identifier. */
  providerZoneRef?: string;
  status: number;
  subdomainCount: Int64String;
  boundSubdomainCount: Int64String;
  verifiedSubdomainCount: Int64String;
  httpsSubdomainCount: Int64String;
  activeDeploymentCount: Int64String;
  createdAt: string;
  updatedAt: string;
}
