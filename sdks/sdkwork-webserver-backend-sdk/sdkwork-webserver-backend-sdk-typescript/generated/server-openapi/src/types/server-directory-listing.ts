import type { PageInfo } from './page-info';
import type { ServerEntry } from './server-entry';

export interface ServerDirectoryListing {
  nodeId: string;
  path: string;
  parentPath: string | null;
  entries: ServerEntry[];
  /** True when the directory has more children than the deployment's maximumEntries cap; the listing is truncated. */
  truncated?: boolean;
  /** Present only when page/pageSize were requested; omitted for the legacy whole-directory response. */
  pageInfo?: PageInfo;
}
