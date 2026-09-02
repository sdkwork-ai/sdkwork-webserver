export interface ServerFileContent {
  nodeId: string;
  path: string;
  /** Decoded text content, bounded by the node read size limit. */
  content: string;
  size: string;
}
