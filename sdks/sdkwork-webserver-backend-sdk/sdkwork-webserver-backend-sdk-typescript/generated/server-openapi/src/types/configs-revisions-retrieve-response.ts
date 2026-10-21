import type { NginxConfigRevisionResponse } from './nginx-config-revision-response';

export interface ConfigsRevisionsRetrieveResponse {
  code: 0;
  data: unknown & { item: NginxConfigRevisionResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
