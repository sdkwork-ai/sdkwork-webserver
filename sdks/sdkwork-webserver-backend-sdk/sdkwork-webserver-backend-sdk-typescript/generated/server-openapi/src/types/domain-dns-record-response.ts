export interface DomainDnsRecordResponse {
  id: string;
  /** Absolute record owner inside the zone, e.g. `www.example.com`. */
  recordName: string;
  /** Zone-relative 主机记录 (`@` for the apex, `www`, `*`). */
  host: string;
  /** Record type as the provider spells it (A, AAAA, CNAME, TXT, MX). */
  recordType: string;
  /** Record content — the resolution IP for A/AAAA, the target for CNAME/MX. */
  recordValue: string;
  ttlSeconds?: number;
  /** Priority for the record types that carry one (MX, SRV). */
  priority?: number;
  /** Provider resolution line, when the provider splits one owner per line. */
  recordLine?: string;
  /** Provider-side resolution state; DISABLED is the vendor's paused record. */
  recordStatus: 'ENABLED' | 'DISABLED';
  /** The registered subdomain this record resolves; absent when its owner matches none. */
  domainId?: string;
  /** Provider family the snapshot was read from. */
  dnsProvider: string;
  /** The cloud account the snapshot was read through. */
  cloudAccountId: string;
  /** Provider-assigned record identity, when the provider returned one. */
  providerRecordRef?: string;
  /** When this row was read from the provider. */
  syncedAt: string;
}
