import type { EnvVariableResponse } from './env-variable-response';

export interface ApplicationsEnvVariablesUpdateResponse {
  code: 0;
  data: unknown & { item: EnvVariableResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
