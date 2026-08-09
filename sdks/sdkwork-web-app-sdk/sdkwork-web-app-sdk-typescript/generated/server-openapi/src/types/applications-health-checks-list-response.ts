import type { HealthCheckResponse } from './health-check-response';
import type { PageInfo } from './page-info';

export interface ApplicationsHealthChecksListResponse {
  code: 0;
  data: unknown & { items: HealthCheckResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
