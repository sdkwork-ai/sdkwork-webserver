import type { PluginResponse } from './plugin-response';

export interface PluginsCreateResponse201 {
  code: 0;
  data: unknown & { item: PluginResponse; };
  /** Server-owned request correlation id. */
  traceId: string;
}
