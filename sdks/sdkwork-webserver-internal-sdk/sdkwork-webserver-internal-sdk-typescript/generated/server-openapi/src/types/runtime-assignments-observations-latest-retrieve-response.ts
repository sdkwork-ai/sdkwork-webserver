import type { RuntimeObservation } from './runtime-observation';

export interface RuntimeAssignmentsObservationsLatestRetrieveResponse {
  code: 0;
  data: unknown & { item: RuntimeObservation; };
  /** Server-owned request correlation id. */
  traceId: string;
}
