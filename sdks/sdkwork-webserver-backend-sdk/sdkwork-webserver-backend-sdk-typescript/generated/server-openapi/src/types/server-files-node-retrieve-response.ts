import type { ServerFileContent } from './server-file-content';

export interface ServerFilesNodeRetrieveResponse {
  code: 0;
  data: unknown & ServerFileContent;
  /** Server-owned request correlation id. */
  traceId: string;
}
