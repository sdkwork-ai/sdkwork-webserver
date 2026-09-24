export interface IssueCertificateRequest {
  /** Ordered exact or wildcard domain identifiers included in the certificate SAN extension. */
  domainIds: string[];
  /** 1=Let's Encrypt, 3=self-signed. Custom import is a separate future workflow. */
  certType: 1 | 3;
  /** Key algorithm of the issued leaf. Defaults to RSA: a managed certificate is renewed unattended and RSA-2048 is the leaf key every TLS client accepts, so a certificate requested without an opinion on this stays reachable from old stacks as well. Ask for ECDSA explicitly where every client is known to support P-256. */
  keyAlgorithm?: 'ECDSA' | 'RSA';
  autoRenew?: boolean;
}
