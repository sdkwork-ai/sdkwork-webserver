export interface CreateHealthCheckRequest {
  checkType: 1 | 2 | 3;
  checkUrl: string;
  checkInterval?: number;
  timeoutMs?: number;
  retryCount?: number;
}
