import type { PlatformTargetResponse } from './platform-target-response';

export interface ApplicationsPlatformTargetsRetrieveResponse {
  code: 0;
  data: unknown & { item: PlatformTargetResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
