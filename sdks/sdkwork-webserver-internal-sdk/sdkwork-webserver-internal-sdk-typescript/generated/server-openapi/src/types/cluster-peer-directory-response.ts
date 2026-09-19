import type { ClusterPeer } from './cluster-peer';

export interface ClusterPeerDirectoryResponse {
  instanceId: string;
  peers: ClusterPeer[];
}
