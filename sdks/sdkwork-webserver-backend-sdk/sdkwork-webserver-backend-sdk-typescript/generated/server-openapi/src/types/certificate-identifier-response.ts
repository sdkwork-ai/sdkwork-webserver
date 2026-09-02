export interface CertificateIdentifierResponse {
  domainId: string;
  hostname: string;
  identifierType: 'EXACT' | 'WILDCARD';
  position: number;
}
