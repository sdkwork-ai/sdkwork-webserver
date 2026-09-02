import type { ApplicationResponse } from './application-response';

export interface ApplicationPage {
  items?: ApplicationResponse[];
  /** Total item count as a string to avoid JavaScript precision loss. */
  total?: string;
  page?: number;
  pageSize?: number;
}
