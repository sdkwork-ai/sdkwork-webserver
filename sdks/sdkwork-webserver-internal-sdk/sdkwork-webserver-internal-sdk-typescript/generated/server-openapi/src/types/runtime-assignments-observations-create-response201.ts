import type { RuntimeObservation } from './runtime-observation';

export interface RuntimeAssignmentsObservationsCreateResponse201 {
  code: 0;
  data: unknown & { item: RuntimeObservation; };
  /** Server-owned request correlation id. */
  traceId: string;
}
