import type { Int64String } from './int64-string';

export interface DomainDnsSyncResponse {
  recordCount: Int64String;
  syncedAt: string;
  /** The zone apex the provider inventory was read for. */
  zoneApex: string;
  dnsProvider: string;
  cloudAccountId: string;
}
