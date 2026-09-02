import type { HealthCheckResponse } from './health-check-response';

export interface ApplicationsHealthChecksCreateResponse201 {
  code: 0;
  data: unknown & { item: HealthCheckResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
