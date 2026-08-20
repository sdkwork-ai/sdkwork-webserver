import type { PageInfo } from './page-info';
import type { PlatformTargetResponse } from './platform-target-response';

export interface ApplicationsPlatformTargetsListResponse {
  code: 0;
  data: unknown & { items: PlatformTargetResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
