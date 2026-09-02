export interface ServerOperationResult {
  operationId: string;
  exitCode?: number | null;
  stdout?: string;
  stderr?: string;
}
