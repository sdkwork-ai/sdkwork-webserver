export interface DnsAccountResponse {
  /** Stable operator-facing reference, and the value `providerAccountId` on an issue request names. */
  accountId: string;
  /** Provider family whose API this account presents challenges through. */
  provider: string;
  /** Hosted zone apex the account can publish into. An identifier is covered when it equals this apex or lives beneath it. */
  zoneApex: string;
}
