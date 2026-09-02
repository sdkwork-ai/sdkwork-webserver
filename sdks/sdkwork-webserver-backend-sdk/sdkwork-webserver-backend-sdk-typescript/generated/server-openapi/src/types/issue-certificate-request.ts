export interface IssueCertificateRequest {
  /** Ordered exact or wildcard domain identifiers included in the certificate SAN extension. */
  domainIds: string[];
  /** 1=Let's Encrypt, 3=self-signed. Custom import is a separate future workflow. */
  certType: 1 | 3;
  keyAlgorithm?: 'ECDSA' | 'RSA';
  autoRenew?: boolean;
}
