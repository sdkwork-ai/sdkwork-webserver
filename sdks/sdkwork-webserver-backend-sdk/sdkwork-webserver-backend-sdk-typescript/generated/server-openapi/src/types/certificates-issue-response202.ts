import type { SdkWorkAsyncData } from './sdk-work-async-data';

export interface CertificatesIssueResponse202 {
  code: 0;
  data: unknown & SdkWorkAsyncData;
  /** Server-owned request correlation id. */
  traceId: string;
}
