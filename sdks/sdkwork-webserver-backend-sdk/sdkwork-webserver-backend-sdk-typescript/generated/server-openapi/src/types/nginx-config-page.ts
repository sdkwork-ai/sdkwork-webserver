import type { NginxConfigResponse } from './nginx-config-response';

export interface NginxConfigPage {
  items?: NginxConfigResponse[];
  /** Total item count as a string to avoid JavaScript precision loss. */
  total?: string;
}
