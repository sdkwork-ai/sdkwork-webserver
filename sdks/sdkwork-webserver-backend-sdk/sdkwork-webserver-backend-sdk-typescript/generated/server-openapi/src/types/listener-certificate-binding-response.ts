import type { ListenerCertificateSummaryResponse } from './listener-certificate-summary-response';

export interface ListenerCertificateBindingResponse {
  id: string;
  siteId: string;
  domainId: string;
  certificateId: string;
  desiredCertificateVersionId: string;
  currentCertificateVersionId?: string;
  desiredCertificate: ListenerCertificateSummaryResponse;
  currentCertificate?: ListenerCertificateSummaryResponse;
  keyAlgorithm: 'ECDSA' | 'RSA';
  priority: number;
  isDefault: boolean;
  status: 'PENDING' | 'DEPLOYING' | 'ACTIVE' | 'PAUSED' | 'FAILED' | 'ARCHIVED';
  activatedAt?: string | null;
  createdAt: string;
  updatedAt: string;
}
