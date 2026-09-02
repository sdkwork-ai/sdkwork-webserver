import type { PlatformTargetResponse } from './platform-target-response';

export interface ApplicationsPlatformTargetsCreateResponse201 {
  code: 0;
  data: unknown & { item: PlatformTargetResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
