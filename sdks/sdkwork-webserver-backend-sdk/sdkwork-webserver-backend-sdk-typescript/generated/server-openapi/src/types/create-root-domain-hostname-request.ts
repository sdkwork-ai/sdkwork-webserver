export interface CreateRootDomainHostnameRequest {
  /** Relative hostname such as @, www, or api.internal. The apex marker `@` resolves to the root domain's own hostname. The single value `*` declares the hostname in wildcard form (`*.example.com`), which is the only shape stored as a `WILDCARD` hostname and therefore the only hostname a certificate with `certificateScope` `WILDCARD` can cover. */
  recordName: string;
  applicationId?: string;
  isPrimary?: boolean;
  sslEnabled?: boolean;
  sslProvider?: 'letsencrypt' | 'custom' | 'none';
}
