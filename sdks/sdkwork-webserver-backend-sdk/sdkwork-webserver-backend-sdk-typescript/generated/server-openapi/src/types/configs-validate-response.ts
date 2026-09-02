import type { NginxValidateResponse } from './nginx-validate-response';

export interface ConfigsValidateResponse {
  code: 0;
  data: unknown & { item: NginxValidateResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
