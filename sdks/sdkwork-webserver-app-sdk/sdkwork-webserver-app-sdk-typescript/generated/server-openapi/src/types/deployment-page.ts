import type { DeploymentResponse } from './deployment-response';

export interface DeploymentPage {
  items?: DeploymentResponse[];
  /** Total item count as a string to avoid JavaScript precision loss. */
  total?: string;
}
