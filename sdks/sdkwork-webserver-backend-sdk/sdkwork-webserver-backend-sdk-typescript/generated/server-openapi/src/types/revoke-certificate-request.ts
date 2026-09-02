export interface RevokeCertificateRequest {
  /** RFC 5280 section 5.3.1 revocation reason. */
  reason: 'keyCompromise' | 'affiliationChanged' | 'superseded' | 'cessationOfOperation' | 'privilegeWithdrawn';
}
