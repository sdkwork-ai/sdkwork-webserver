import type { NginxDeployResponse } from './nginx-deploy-response';

export interface ConfigsDeployResponse {
  code: 0;
  data: unknown & { item: NginxDeployResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
