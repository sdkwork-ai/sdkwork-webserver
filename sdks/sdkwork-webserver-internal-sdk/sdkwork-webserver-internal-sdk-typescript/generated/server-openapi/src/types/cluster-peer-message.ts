export interface ClusterPeerMessage {
  id: string;
  messageType: string;
  fromInstanceId?: string;
  toInstanceId?: string;
  payload: Record<string, unknown>;
  createdAt: string;
}
