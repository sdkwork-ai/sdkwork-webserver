import type { ServerFileContent } from './server-file-content';

export interface ServerFilesNodeReadResponse {
  code: 0;
  data: unknown & ServerFileContent;
  /** Server-owned request correlation id. */
  traceId: string;
}
