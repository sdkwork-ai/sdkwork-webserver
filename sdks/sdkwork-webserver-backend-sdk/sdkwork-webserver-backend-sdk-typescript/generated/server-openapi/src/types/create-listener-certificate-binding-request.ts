export interface CreateListenerCertificateBindingRequest {
  certificateId: string;
  /** Immutable certificate version. Omit to bind the certificate's current active version. */
  certificateVersionId?: string;
  priority?: number;
  isDefault?: boolean;
}
