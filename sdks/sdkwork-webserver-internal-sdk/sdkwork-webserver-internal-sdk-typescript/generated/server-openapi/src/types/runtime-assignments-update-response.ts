import type { RuntimeAssignment } from './runtime-assignment';

export interface RuntimeAssignmentsUpdateResponse {
  code: 0;
  data: unknown & { item: RuntimeAssignment; };
  /** Server-owned request correlation id. */
  traceId: string;
}
