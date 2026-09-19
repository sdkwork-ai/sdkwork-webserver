import type { Int64String } from './int64-string';

export interface EnqueueClusterPeerMessagesResponse {
  /** Rows materialized (one per target member). */
  enqueued: Int64String;
}
