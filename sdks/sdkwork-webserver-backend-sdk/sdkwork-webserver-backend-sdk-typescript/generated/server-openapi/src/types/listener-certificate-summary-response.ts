import type { CertificateIdentifierResponse } from './certificate-identifier-response';

export interface ListenerCertificateSummaryResponse {
  certName: string;
  identifiers: CertificateIdentifierResponse[];
  issuer?: string;
  fingerprint?: string;
  notAfter?: string;
  status: 'PENDING' | 'ISSUED' | 'FAILED' | 'EXPIRED' | 'REVOKED' | 'ARCHIVED';
}
