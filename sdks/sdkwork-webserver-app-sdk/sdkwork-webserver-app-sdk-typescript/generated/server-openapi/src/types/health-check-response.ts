export interface HealthCheckResponse {
  id: string;
  checkType: number;
  checkUrl: string;
  checkInterval: number;
  timeoutMs: number;
  retryCount: number;
  status: number;
  createdAt: string;
}
