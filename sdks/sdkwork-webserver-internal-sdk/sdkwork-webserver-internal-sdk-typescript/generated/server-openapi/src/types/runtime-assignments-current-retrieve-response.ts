import type { RuntimeAssignmentDelivery } from './runtime-assignment-delivery';

export interface RuntimeAssignmentsCurrentRetrieveResponse {
  code: 0;
  data: unknown & { item: RuntimeAssignmentDelivery; };
  /** Server-owned request correlation id. */
  traceId: string;
}
