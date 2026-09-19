import type { ClusterHostDescriptor } from './cluster-host-descriptor';
import type { ClusterInstanceDescriptor } from './cluster-instance-descriptor';

export interface ClusterRegistrationRequest {
  /** Target cluster code; omitted registers into the auto-provisioned default cluster. */
  clusterCode?: string;
  host: ClusterHostDescriptor;
  instance: ClusterInstanceDescriptor;
}
