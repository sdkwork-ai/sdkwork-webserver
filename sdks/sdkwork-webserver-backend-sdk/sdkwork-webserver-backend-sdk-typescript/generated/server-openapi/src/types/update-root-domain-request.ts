/** Partial edit; an omitted member leaves the stored value unchanged. The apex hostname is not editable. */
export interface UpdateRootDomainRequest {
  displayName?: string;
  dnsProvider?: string;
  providerZoneRef?: string;
  /** 0=pending, 1=active, 2=disabled. */
  status?: number;
}
