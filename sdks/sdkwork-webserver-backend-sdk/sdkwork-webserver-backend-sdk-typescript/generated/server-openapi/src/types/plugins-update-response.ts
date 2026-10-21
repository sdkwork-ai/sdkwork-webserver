import type { PluginResponse } from './plugin-response';

export interface PluginsUpdateResponse {
  code: 0;
  data: unknown & { item: PluginResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
