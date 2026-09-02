import type { AuditLogResponse } from './audit-log-response';
import type { PageInfo } from './page-info';

export interface AuditLogsListResponse {
  code: 0;
  data: unknown & { items: AuditLogResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
