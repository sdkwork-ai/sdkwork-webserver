import type { Int64String } from './int64-string';

/** One app's aggregate over the window. A row carrying neither `appUuid` nor `appSlug` is the **unattributed** bucket: traffic served for a hostname the edge could not resolve to an app. It is reported rather than dropped so the per-app rows keep summing back to the total; a surface must render it as its own row, or the breakdown appears to lose traffic. */
export interface TrafficUsageAppTotal {
  appUuid?: string;
  appSlug?: string;
  dimension: string;
  quantity: Int64String;
  unit: string;
}
