export interface WebserverConfigWriteRequest {
  /** New text content; NUL-free, within the write size limit, and syntax-validated for JSON/TOML files. */
  content: string;
  /** When provided, the write is rejected unless the on-disk digest still matches (optimistic concurrency). */
  expectedSha256?: string;
}
