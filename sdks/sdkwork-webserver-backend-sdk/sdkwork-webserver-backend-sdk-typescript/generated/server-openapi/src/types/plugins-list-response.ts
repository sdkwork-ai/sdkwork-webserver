import type { PageInfo } from './page-info';
import type { PluginResponse } from './plugin-response';

export interface PluginsListResponse {
  code: 0;
  data: unknown & { items: PluginResponse[]; pageInfo: PageInfo; };
  /** Server-owned request correlation id. */
  traceId: string;
}
