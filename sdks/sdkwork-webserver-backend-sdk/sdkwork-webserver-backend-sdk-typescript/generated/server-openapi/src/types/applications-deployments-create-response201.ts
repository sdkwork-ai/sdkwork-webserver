import type { ApplicationDeploymentResponse } from './application-deployment-response';

export interface ApplicationsDeploymentsCreateResponse201 {
  code: 0;
  data: unknown & { item: ApplicationDeploymentResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
