export interface EnqueueClusterPeerMessagesRequest {
  clusterId: string;
  /** Target instance; omitted broadcasts to every online member. */
  toInstanceId?: string;
  /** Sender instance; omitted for control-plane-originated messages. */
  fromInstanceId?: string;
  messageType: string;
  /** Message payload JSON; bounded to 16 KiB. */
  payload?: Record<string, unknown>;
  expiresInSeconds?: number;
}
