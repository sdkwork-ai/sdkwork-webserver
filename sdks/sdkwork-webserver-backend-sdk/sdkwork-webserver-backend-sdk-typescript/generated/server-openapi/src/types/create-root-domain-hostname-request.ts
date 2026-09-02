export interface CreateRootDomainHostnameRequest {
  /** Relative hostname such as @, www, or api.internal. */
  recordName: string;
  applicationId?: string;
  isPrimary?: boolean;
  sslEnabled?: boolean;
  sslProvider?: 'letsencrypt' | 'custom' | 'none';
}
