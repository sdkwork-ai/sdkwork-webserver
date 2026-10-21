import type { ServerEntry } from './server-entry';

export interface ServerDirectoryListing {
  nodeId: string;
  path: string;
  parentPath: string | null;
  entries: ServerEntry[];
  /** True when the directory has more children than the deployment's maximumEntries cap; the listing is truncated. */
  truncated?: boolean;
}
