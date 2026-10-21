import type { PageInfo } from './page-info';
import type { WebserverConfigEntry } from './webserver-config-entry';

export interface WebserverConfigCatalog {
  /** Web Server runtime configuration root the catalog was enumerated from. */
  configRoot: string;
  items: WebserverConfigEntry[];
  /** True when the deployment has more catalog entries than the server's maximumEntries cap; items is the capped prefix. */
  truncated?: boolean;
  /** Present only when page/pageSize were requested; omitted for the legacy whole-catalog response. */
  pageInfo?: PageInfo;
}
