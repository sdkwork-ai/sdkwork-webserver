import type { SourceVersionResponse } from './source-version-response';

export interface ApplicationsSourceVersionsRetrieveResponse {
  code: 0;
  data: unknown & { item: SourceVersionResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
