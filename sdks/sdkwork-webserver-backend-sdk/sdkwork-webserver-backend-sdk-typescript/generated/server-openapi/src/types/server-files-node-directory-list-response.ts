import type { ServerDirectoryListing } from './server-directory-listing';

export interface ServerFilesNodeDirectoryListResponse {
  code: 0;
  data: unknown & ServerDirectoryListing;
  /** Server-owned request correlation id. */
  traceId: string;
}
