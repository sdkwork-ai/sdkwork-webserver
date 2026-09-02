import type { DeploymentResponse } from './deployment-response';
import type { PageInfo } from './page-info';

export interface ApplicationsDeploymentsListResponse {
  code: 0;
  data: unknown & { items: DeploymentResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
