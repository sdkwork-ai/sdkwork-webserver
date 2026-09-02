import type { CertificateIdentifierResponse } from './certificate-identifier-response';

export interface CertificateResponse {
  id: string;
  certName: string;
  identifiers: CertificateIdentifierResponse[];
  certType?: number;
  issuer?: string;
  fingerprint?: string;
  keyAlgorithm: 'ECDSA' | 'RSA';
  notBefore?: string;
  notAfter?: string;
  autoRenew?: boolean;
  renewalStatus?: 'IDLE' | 'RENEWING' | 'PENDING' | 'FAILED';
  status: 'PENDING' | 'ISSUED' | 'FAILED' | 'EXPIRED' | 'REVOKED' | 'ARCHIVED';
  createdAt: string;
}
