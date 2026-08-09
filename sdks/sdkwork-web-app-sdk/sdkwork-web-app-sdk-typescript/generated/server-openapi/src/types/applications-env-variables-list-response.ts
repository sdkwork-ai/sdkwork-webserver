import type { EnvVariableResponse } from './env-variable-response';
import type { PageInfo } from './page-info';

export interface ApplicationsEnvVariablesListResponse {
  code: 0;
  data: unknown & { items: EnvVariableResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
