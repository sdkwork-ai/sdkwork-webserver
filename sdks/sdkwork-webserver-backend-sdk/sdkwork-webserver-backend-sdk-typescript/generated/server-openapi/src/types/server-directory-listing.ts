import type { ServerEntry } from './server-entry';

export interface ServerDirectoryListing {
  nodeId: string;
  path: string;
  parentPath: string | null;
  entries: ServerEntry[];
}
