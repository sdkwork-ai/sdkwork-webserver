import type { ServerResponse } from './server-response';

export interface ServerPage {
  items?: ServerResponse[];
  /** Total item count as a string to avoid JavaScript precision loss. */
  total?: string;
}
