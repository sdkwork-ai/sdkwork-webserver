export interface CreateApplicationDomainRequest {
  hostname: string;
  isPrimary?: boolean;
  sslEnabled?: boolean;
  sslProvider?: 'letsencrypt' | 'custom' | 'none';
}
