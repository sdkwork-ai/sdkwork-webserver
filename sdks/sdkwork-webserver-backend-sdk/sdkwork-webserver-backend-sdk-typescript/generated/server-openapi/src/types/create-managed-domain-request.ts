export interface CreateManagedDomainRequest {
  hostname: string;
  applicationId?: string;
  isPrimary?: boolean;
  sslEnabled?: boolean;
  sslProvider?: 'letsencrypt' | 'custom' | 'none';
}
