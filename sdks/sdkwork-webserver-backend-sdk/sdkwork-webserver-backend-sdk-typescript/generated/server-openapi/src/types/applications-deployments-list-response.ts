import type { ApplicationDeploymentResponse } from './application-deployment-response';
import type { PageInfo } from './page-info';

export interface ApplicationsDeploymentsListResponse {
  code: 0;
  data: unknown & { items: ApplicationDeploymentResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
