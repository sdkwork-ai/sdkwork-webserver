import type { HealthCheckResponse } from './health-check-response';

export interface HealthCheckPage {
  items?: HealthCheckResponse[];
  /** Total item count as a string to avoid JavaScript precision loss. */
  total?: string;
}
