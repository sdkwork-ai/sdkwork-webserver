import type { ServerDirectoryListing } from './server-directory-listing';

export interface ServerFilesNodeBrowseResponse {
  code: 0;
  data: unknown & ServerDirectoryListing;
  /** Server-owned request correlation id. */
  traceId: string;
}
