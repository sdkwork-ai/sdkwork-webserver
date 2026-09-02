import type { AuditLogResponse } from './audit-log-response';

export interface AuditLogPage {
  items?: AuditLogResponse[];
  /** Total item count as a string to avoid JavaScript precision loss. */
  total?: string;
}
