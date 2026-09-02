import type { DomainResponse } from './domain-response';

export interface DomainPage {
  items?: DomainResponse[];
  /** Total item count as a string to avoid JavaScript precision loss. */
  total?: string;
}
