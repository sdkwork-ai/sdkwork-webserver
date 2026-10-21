export interface DomainWechatVerificationCheckResponse {
  /** Whether any protocol fetched a 2xx response. */
  reachable: boolean;
  /** Whether the fetched content matches the stored file byte for byte. */
  matched: boolean;
  /** HTTP status code of the matched attempt. */
  statusCode?: number;
  /** Matched scheme (https or http). */
  scheme?: string;
  /** One-line fact when the check did not pass. */
  detail?: string;
  /** Time of the check. */
  checkedAt: string;
}
