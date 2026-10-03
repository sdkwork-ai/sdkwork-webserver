/** Partial edit; an omitted member leaves the stored value unchanged. The apex hostname is not editable. */
export interface UpdateRootDomainRequest {
  displayName?: string;
  dnsProvider?: string;
  providerZoneRef?: string;
  /** 0=pending, 1=active, 2=disabled. */
  status?: number;
  /** The cloud account to bind, or `null` to unbind. The **only** member of this request that distinguishes "leave it alone" from "clear it": an omitted member keeps the stored account and an explicit `null` removes it. This is the one place the surface's "a blank value is an omission" reading does not apply, because the member names an association rather than describing the row. */
  cloudAccountId?: string | null;
}
