export interface DomainResponse {
  id: string;
  hostname: string;
  applicationId?: string;
  applicationName?: string;
  certificateCount: string;
  isPrimary: boolean;
  isVerified: boolean;
  sslEnabled: boolean;
  sslProvider?: string;
  status: number;
  createdAt: string;
}
