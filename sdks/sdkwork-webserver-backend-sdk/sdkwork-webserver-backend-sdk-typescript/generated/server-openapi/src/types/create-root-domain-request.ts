export interface CreateRootDomainRequest {
  hostname: string;
  /** Cloud account whose DNS automation this root domain is bound to. Omitted leaves the Zone resolving its account per operation, which is the state every root domain reconciled from the edge's own configuration is in. */
  cloudAccountId?: string;
}
