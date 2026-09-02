export interface AuditLogResponse {
  id?: string;
  /** Operator user id as a string to avoid JavaScript precision loss. */
  operatorId?: string;
  operatorType?: string;
  action?: string;
  targetType?: string;
  /** Target snowflake id as a string to avoid JavaScript precision loss. Null when the audit action has no specific numeric target. */
  targetId?: string;
  targetUuid?: string | null;
  ipAddress?: string | null;
  changes?: Record<string, unknown>;
  createdAt?: string;
}
