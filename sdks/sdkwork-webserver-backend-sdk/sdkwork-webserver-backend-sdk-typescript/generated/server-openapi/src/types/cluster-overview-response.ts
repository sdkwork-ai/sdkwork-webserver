import type { Int64String } from './int64-string';

export interface ClusterOverviewResponse {
  totalHosts: Int64String;
  onlineHosts: Int64String;
  totalInstances: Int64String;
  onlineInstances: Int64String;
  unhealthyInstances: Int64String;
  pendingPeerMessages: Int64String;
  generatedAt: string;
}
