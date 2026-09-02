export interface SdkWorkAsyncData {
  accepted: true;
  operationId: string;
  status: 'pending' | 'running' | 'succeeded' | 'failed' | 'cancelled';
  pollUrl?: string;
}
