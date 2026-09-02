import type { CreateServerResponse } from './create-server-response';

export interface ServersCreateResponse201 {
  code: 0;
  data: unknown & { item: CreateServerResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
